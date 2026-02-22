#!/usr/bin/env python3
"""
Open Agent Economy simulation for Microstable Phase 2.
Pure Python (numpy optional). Python 3.12+ compatible.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, ClassVar, Dict, List, Optional, Set, Tuple
import hashlib
import hmac
import json
import math
import random
import time

try:
    import numpy as _np  # type: ignore

    HAS_NUMPY = True
except Exception:
    _np = None
    HAS_NUMPY = False


EPS = 1e-9

AGENT_TYPES = ("Optimizer", "Monitor", "Auditor", "Liquidator")
AGENT_STATUSES = ("Active", "Cooldown", "Slashed", "Deregistered")

MIN_STAKE_DEFAULT = {
    "Optimizer": 10.0,
    "Monitor": 5.0,
    "Auditor": 20.0,
    "Liquidator": 2.0,
}

REPUTATION_TIERS = [
    ("Newcomer", 0, 99, 1.0),
    ("Established", 100, 499, 1.5),
    ("Veteran", 500, 999, 2.0),
    ("Elite", 1000, 10_000_000, 3.0),
]


# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------


def sha256_hex(payload: str) -> str:
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def hmac_sha256_hex(key: str, payload: str) -> str:
    return hmac.new(key.encode("utf-8"), payload.encode("utf-8"), hashlib.sha256).hexdigest()


def canonical_json(data: Any) -> str:
    return json.dumps(data, sort_keys=True, separators=(",", ":"))


def safe_cosine_similarity(a: List[float], b: List[float]) -> float:
    if not a or not b:
        return 0.0
    if len(a) != len(b):
        return 0.0
    if HAS_NUMPY:
        va = _np.array(a, dtype=float)
        vb = _np.array(b, dtype=float)
        denom = float(_np.linalg.norm(va) * _np.linalg.norm(vb))
        if denom <= EPS:
            return 0.0
        return float(_np.dot(va, vb) / denom)
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    denom = na * nb
    if denom <= EPS:
        return 0.0
    return dot / denom


def gini(values: List[float]) -> float:
    if not values:
        return 0.0
    sorted_vals = sorted(values)
    n = len(sorted_vals)
    cum = 0.0
    for i, v in enumerate(sorted_vals, 1):
        cum += i * v
    total = sum(sorted_vals)
    if total <= EPS:
        return 0.0
    return (2.0 * cum) / (n * total) - (n + 1) / n


def percentile(values: List[float], p: float) -> float:
    if not values:
        return 0.0
    vals = sorted(values)
    k = (len(vals) - 1) * p
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return vals[int(k)]
    d0 = vals[int(f)] * (c - k)
    d1 = vals[int(c)] * (k - f)
    return d0 + d1


def now_ms() -> int:
    return int(time.time() * 1000)


# -----------------------------------------------------------------------------
# Agent Registry
# -----------------------------------------------------------------------------


@dataclass
class AgentRecord:
    agent_id: str
    agent_type: str
    stake: float
    reputation: int
    registered_at: int
    last_active: int
    total_rewards: float
    total_slashed: float
    proposals_submitted: int
    proposals_accepted: int
    status: str


class AgentRegistry:
    def __init__(
        self,
        min_stake_by_type: Optional[Dict[str, float]] = None,
        max_agents_per_type: Optional[Dict[str, int]] = None,
        cooldown_epochs: int = 5,
        require_challenge_exam: bool = False,
        challenge_exam_checker: Optional[Callable[[str, str, float], bool]] = None,
    ) -> None:
        self.min_stake_by_type = min_stake_by_type or dict(MIN_STAKE_DEFAULT)
        self.max_agents_per_type = max_agents_per_type or {}
        self.cooldown_epochs = cooldown_epochs
        self.require_challenge_exam = require_challenge_exam
        self.challenge_exam_checker = challenge_exam_checker
        self.records: Dict[str, AgentRecord] = {}
        self.cooldowns: Dict[str, int] = {}
        self.meta: Dict[str, Dict[str, Any]] = {}

    def _now(self, epoch: Optional[int]) -> int:
        return int(epoch if epoch is not None else 0)

    def configure_intelligence_gate(
        self,
        required: bool,
        checker: Optional[Callable[[str, str, float], bool]] = None,
    ) -> None:
        """Enable/disable Challenge Exam admission checks.

        When enabled, register() requires either:
        - challenge_exam_passed=True, or
        - checker callback returns True.
        """
        self.require_challenge_exam = bool(required)
        self.challenge_exam_checker = checker

    def register(
        self,
        agent_id: str,
        agent_type: str,
        stake: float,
        epoch: Optional[int] = None,
        challenge_exam_passed: Optional[bool] = None,
    ) -> bool:
        if agent_type not in AGENT_TYPES:
            return False
        if self.require_challenge_exam:
            passed = challenge_exam_passed
            if passed is None and self.challenge_exam_checker is not None:
                passed = bool(self.challenge_exam_checker(agent_id, agent_type, float(stake)))
            if not passed:
                return False
        min_stake = self.min_stake_by_type.get(agent_type, 0.0)
        if stake < min_stake:
            return False
        if agent_id in self.records and self.records[agent_id].status != "Deregistered":
            return False
        max_allowed = self.max_agents_per_type.get(agent_type)
        if max_allowed is not None:
            active_count = len([r for r in self.records.values() if r.agent_type == agent_type and r.status == "Active"])
            if active_count >= max_allowed:
                return False
        now = self._now(epoch)
        self.records[agent_id] = AgentRecord(
            agent_id=agent_id,
            agent_type=agent_type,
            stake=float(stake),
            reputation=0,
            registered_at=now,
            last_active=now,
            total_rewards=0.0,
            total_slashed=0.0,
            proposals_submitted=0,
            proposals_accepted=0,
            status="Active",
        )
        if agent_id in self.cooldowns:
            self.cooldowns.pop(agent_id, None)
        return True

    def deregister(self, agent_id: str, epoch: Optional[int] = None) -> bool:
        if agent_id not in self.records:
            return False
        rec = self.records[agent_id]
        if rec.status != "Active":
            return False
        now = self._now(epoch)
        rec.status = "Cooldown"
        rec.last_active = now
        self.cooldowns[agent_id] = now + self.cooldown_epochs
        return True

    def finalize_deregistration(self, agent_id: str, epoch: Optional[int] = None) -> bool:
        if agent_id not in self.records:
            return False
        rec = self.records[agent_id]
        if rec.status != "Cooldown":
            return False
        now = self._now(epoch)
        if now < self.cooldowns.get(agent_id, now + 1):
            return False
        rec.status = "Deregistered"
        rec.last_active = now
        self.cooldowns.pop(agent_id, None)
        return True

    def slash(self, agent_id: str, amount: float, epoch: Optional[int] = None) -> float:
        if agent_id not in self.records:
            return 0.0
        rec = self.records[agent_id]
        now = self._now(epoch)
        slash_amt = rec.stake * amount if amount <= 1.0 else amount
        slash_amt = min(slash_amt, rec.stake)
        rec.stake -= slash_amt
        rec.total_slashed += slash_amt
        rec.last_active = now
        min_stake = self.min_stake_by_type.get(rec.agent_type, 0.0)
        # Ratio-based penalties are temporary (Slashed), while large absolute
        # penalties can force deregistration if stake falls below minimum.
        if amount <= 1.0:
            rec.status = "Deregistered" if rec.stake <= EPS else "Slashed"
        elif rec.stake < min_stake:
            rec.status = "Deregistered"
        else:
            rec.status = "Slashed"
        return slash_amt

    def reward(self, agent_id: str, amount: float, epoch: Optional[int] = None) -> bool:
        if agent_id not in self.records:
            return False
        rec = self.records[agent_id]
        now = self._now(epoch)
        rec.stake += amount
        rec.total_rewards += amount
        rec.last_active = now
        if rec.status == "Slashed" and rec.stake >= self.min_stake_by_type.get(rec.agent_type, 0.0):
            rec.status = "Active"
        return True

    def heartbeat(self, agent_id: str, epoch: Optional[int] = None) -> bool:
        if agent_id not in self.records:
            return False
        self.records[agent_id].last_active = self._now(epoch)
        return True

    def touch(self, agent_id: str, epoch: Optional[int] = None) -> None:
        if agent_id in self.records:
            self.records[agent_id].last_active = self._now(epoch)

    def set_meta(self, agent_id: str, **kwargs: Any) -> None:
        if agent_id not in self.meta:
            self.meta[agent_id] = {}
        self.meta[agent_id].update(kwargs)

    def set_public_key(self, agent_id: str, public_key: str) -> None:
        """Register per-agent ACP verification key (Ed25519-like public key slot)."""
        # FIX PT-012: bind ACP verification to per-agent registry key, not shared secret.
        self.set_meta(agent_id, public_key=str(public_key))

    def get_public_key(self, agent_id: str) -> Optional[str]:
        return str(self.meta.get(agent_id, {}).get("public_key")) if agent_id in self.meta and "public_key" in self.meta[agent_id] else None

    def get_record(self, agent_id: str) -> Optional[AgentRecord]:
        return self.records.get(agent_id)

    def active_agents(self, agent_type: Optional[str] = None) -> List[AgentRecord]:
        return [
            r
            for r in self.records.values()
            if r.status == "Active" and (agent_type is None or r.agent_type == agent_type)
        ]

    def can_submit(self, agent_id: str) -> bool:
        rec = self.records.get(agent_id)
        return rec is not None and rec.status == "Active"


# -----------------------------------------------------------------------------
# Reputation
# -----------------------------------------------------------------------------


class ReputationEngine:
    def __init__(self) -> None:
        self.scores: Dict[str, int] = {}
        self.last_active: Dict[str, int] = {}
        self.history: Dict[str, List[int]] = {}

    def get(self, agent_id: str) -> int:
        return self.scores.get(agent_id, 0)

    def add(self, agent_id: str, delta: int) -> int:
        cur = self.scores.get(agent_id, 0)
        cur += delta
        cur = max(cur, -1000)
        self.scores[agent_id] = cur
        self.history.setdefault(agent_id, []).append(cur)
        return cur

    def rate_limited_add(self, agent_id: str, delta: int, max_per_epoch: int) -> int:
        if abs(delta) > max_per_epoch:
            delta = max_per_epoch if delta > 0 else -max_per_epoch
        return self.add(agent_id, delta)

    def update_activity(self, agent_id: str, epoch: int) -> None:
        self.last_active[agent_id] = epoch

    def apply_decay(self, agent_id: str, weeks_inactive: int) -> int:
        cur = self.scores.get(agent_id, 0)
        for _ in range(weeks_inactive):
            cur = int(math.floor(cur * 0.99))
        cur = max(cur, -1000)
        self.scores[agent_id] = cur
        return cur

    def tier(self, agent_id: str) -> str:
        score = self.get(agent_id)
        for name, low, high, _ in REPUTATION_TIERS:
            if low <= score <= high:
                return name
        return "Newcomer"

    def multiplier(self, agent_id: str) -> float:
        score = self.get(agent_id)
        for name, low, high, mult in REPUTATION_TIERS:
            if low <= score <= high:
                return mult
        return 1.0


# -----------------------------------------------------------------------------
# Staking & Economics
# -----------------------------------------------------------------------------


class StakingEconomics:
    def __init__(
        self,
        registry: AgentRegistry,
        cooldown_epochs: int = 5,
        reward_epoch_cap: float = 1_000.0,
        claim_signing_key: str = "OAE_REWARD_AUTHORITY_V1",
        legacy_unsigned_claim_limit: float = 1.0,
    ) -> None:
        self.registry = registry
        self.cooldown_epochs = cooldown_epochs
        self.reward_epoch_cap = max(0.0, float(reward_epoch_cap))
        self.claim_signing_key = str(claim_signing_key)
        self.legacy_unsigned_claim_limit = max(0.0, float(legacy_unsigned_claim_limit))

        self.balances: Dict[str, float] = {}
        self.locked: Dict[str, float] = {}
        self.pending: Dict[str, Tuple[float, int]] = {}
        self.total_deposited: float = 0.0
        self.total_rewards: float = 0.0
        self.total_slashed: float = 0.0
        self.claimed: set[str] = set()
        self.claimed_by_epoch: Dict[int, float] = {}

    def deposit(self, agent_id: str, agent_type: str, amount: float, epoch: int) -> bool:
        min_stake = self.registry.min_stake_by_type.get(agent_type, 0.0)
        if amount < min_stake and agent_id not in self.balances:
            return False
        self.balances[agent_id] = self.balances.get(agent_id, 0.0) + amount
        self.total_deposited += amount
        return True

    def available(self, agent_id: str) -> float:
        return self.balances.get(agent_id, 0.0) - self.locked.get(agent_id, 0.0)

    def lock(self, agent_id: str, amount: float) -> bool:
        if self.available(agent_id) < amount:
            return False
        self.locked[agent_id] = self.locked.get(agent_id, 0.0) + amount
        return True

    def unlock(self, agent_id: str, amount: float) -> None:
        self.locked[agent_id] = max(0.0, self.locked.get(agent_id, 0.0) - amount)

    def request_withdrawal(self, agent_id: str, amount: float, epoch: int) -> bool:
        if amount <= 0.0:
            return False
        if agent_id in self.pending:
            return False
        if self.available(agent_id) < amount:
            return False
        # FIX PT-002: lock withdrawal amount at request time.
        self.locked[agent_id] = self.locked.get(agent_id, 0.0) + amount
        self.pending[agent_id] = (amount, epoch + self.cooldown_epochs)
        return True

    def withdraw(self, agent_id: str, epoch: int) -> float:
        if agent_id not in self.pending:
            raise ValueError("no pending withdrawal")
        requested, unlock_epoch = self.pending[agent_id]
        if epoch < unlock_epoch:
            raise ValueError("cooldown not finished")

        balance = self.balances.get(agent_id, 0.0)
        locked = self.locked.get(agent_id, 0.0)
        withdrawable = max(0.0, min(requested, balance, locked))
        self.pending.pop(agent_id, None)
        self.unlock(agent_id, requested)
        self.balances[agent_id] = max(0.0, balance - withdrawable)
        return withdrawable

    def slash(self, agent_id: str, amount: float, epoch: int) -> float:
        balance = self.balances.get(agent_id, 0.0)
        locked_before = self.locked.get(agent_id, 0.0)
        available_before = max(0.0, balance - locked_before)

        slash_amt = balance * amount if amount <= 1.0 else amount
        slash_amt = min(slash_amt, balance)
        self.balances[agent_id] = balance - slash_amt

        # FIX PT-002: slash also burns locked withdrawal capacity when needed.
        overflow_to_locked = max(0.0, slash_amt - available_before)
        if overflow_to_locked > 0.0:
            self.locked[agent_id] = max(0.0, locked_before - overflow_to_locked)

        if agent_id in self.pending:
            pending_amt, unlock_epoch = self.pending[agent_id]
            effective_locked = self.locked.get(agent_id, 0.0)
            if pending_amt > effective_locked:
                self.pending[agent_id] = (effective_locked, unlock_epoch)

        self.total_slashed += slash_amt
        rec = self.registry.get_record(agent_id)
        if rec:
            self.registry.slash(agent_id, slash_amt, epoch)
        return slash_amt

    def reward(self, agent_id: str, amount: float, epoch: int) -> None:
        self.balances[agent_id] = self.balances.get(agent_id, 0.0) + amount
        self.total_rewards += amount
        self.registry.reward(agent_id, amount, epoch)

    def _reward_claim_payload(self, agent_id: str, amount: float, claim_id: str, epoch: int) -> str:
        payload = {
            "agent_id": str(agent_id),
            "amount": float(amount),
            "claim_id": str(claim_id),
            "epoch": int(epoch),
        }
        return canonical_json(payload)

    def build_claim_proof(self, agent_id: str, amount: float, claim_id: str, epoch: int) -> str:
        """Create signed reward-evidence proof for claim_reward."""
        return hmac_sha256_hex(self.claim_signing_key, self._reward_claim_payload(agent_id, amount, claim_id, epoch))

    def claim_reward(self, agent_id: str, amount: float, claim_id: str, epoch: int, proof: Optional[str] = None) -> bool:
        if amount <= 0.0 or claim_id in self.claimed:
            return False

        used = self.claimed_by_epoch.get(epoch, 0.0)
        # FIX PT-001: enforce strict epoch reward budget cap.
        if used + amount > self.reward_epoch_cap + EPS:
            return False

        expected = self.build_claim_proof(agent_id, amount, claim_id, epoch)
        has_valid_proof = proof is not None and hmac.compare_digest(str(proof), expected)

        # FIX PT-001: require signed evidence for meaningful claims.
        if not has_valid_proof and amount > self.legacy_unsigned_claim_limit:
            return False

        self.claimed.add(claim_id)
        self.claimed_by_epoch[epoch] = used + amount
        self.reward(agent_id, amount, epoch)
        return True

    def apy(self, total_epochs: int, epochs_per_year: int = 8760) -> float:
        principal = self.total_deposited
        if principal <= EPS or total_epochs <= 0:
            return 0.0
        return (self.total_rewards / principal) * (epochs_per_year / total_epochs)

    def invariant(self) -> float:
        total_balance = sum(self.balances.values())
        return (total_balance + self.total_slashed) - (self.total_deposited + self.total_rewards)


# -----------------------------------------------------------------------------
# ACP Message + Rate limiting
# -----------------------------------------------------------------------------


@dataclass
class ACPMessage:
    jsonrpc: str
    method: str
    params: Dict[str, Any]
    id: str

    _seen_nonces: ClassVar[Set[str]] = set()

    @staticmethod
    def _payload(method: str, params: Dict[str, Any], msg_id: str) -> str:
        return canonical_json({"method": method, "params": params, "id": msg_id})

    @staticmethod
    def sign(method: str, params: Dict[str, Any], msg_id: str, secret: str) -> str:
        return hmac_sha256_hex(secret, ACPMessage._payload(method, params, msg_id))

    @staticmethod
    def create(
        method: str,
        params: Dict[str, Any],
        msg_id: Optional[str],
        secret: str,
        *,
        epoch: int = 0,
        expiry_epoch: Optional[int] = None,
        nonce: Optional[str] = None,
    ) -> "ACPMessage":
        msg_id = msg_id or sha256_hex(f"{method}:{now_ms()}:{random.random()}")[:12]
        payload_params = dict(params)
        # FIX PT-011: bind nonce/expiry/epoch into signed ACP payload.
        payload_params.setdefault("epoch", int(epoch))
        payload_params.setdefault("expiry_epoch", int(expiry_epoch if expiry_epoch is not None else int(payload_params["epoch"]) + 2))
        payload_params.setdefault("nonce", str(nonce or sha256_hex(f"{msg_id}:{now_ms()}:{random.random()}")[:16]))

        signature = ACPMessage.sign(method, payload_params, msg_id, secret)
        payload_params["signature"] = signature
        return ACPMessage(jsonrpc="2.0", method=method, params=payload_params, id=msg_id)

    @staticmethod
    def _select_verification_key(
        msg: "ACPMessage",
        secret: Optional[str],
        registry: Optional[AgentRegistry],
    ) -> Optional[str]:
        agent_id = str(msg.params.get("agent_id", ""))
        # FIX PT-012: prefer per-agent registry key over shared secret.
        if registry is not None and agent_id:
            pub = registry.get_public_key(agent_id)
            if pub:
                return pub
        return secret

    @staticmethod
    def verify(
        msg: "ACPMessage",
        secret: Optional[str] = None,
        *,
        registry: Optional[AgentRegistry] = None,
        now_epoch: Optional[int] = None,
        expected_epoch: Optional[int] = None,
        seen_nonces: Optional[Set[str]] = None,
        max_future_drift: int = 1,
        allow_legacy: bool = True,
    ) -> bool:
        params = dict(msg.params)
        signature = params.pop("signature", None)
        if not isinstance(signature, str):
            return False

        key = ACPMessage._select_verification_key(msg, secret, registry)
        if not key:
            return False

        expected = ACPMessage.sign(msg.method, params, msg.id, key)
        if not hmac.compare_digest(signature, expected):
            return False

        nonce = params.get("nonce")
        expiry = params.get("expiry_epoch")
        signed_epoch = params.get("epoch")
        has_replay_fields = nonce is not None and expiry is not None and signed_epoch is not None

        if not has_replay_fields:
            return bool(allow_legacy)

        try:
            nonce_s = str(nonce)
            expiry_i = int(expiry)
            signed_epoch_i = int(signed_epoch)
        except Exception:
            return False

        if expected_epoch is not None and signed_epoch_i != int(expected_epoch):
            return False

        if now_epoch is not None:
            # FIX PT-009: reject impossible far-future signed epochs.
            if signed_epoch_i > int(now_epoch) + int(max_future_drift):
                return False
            if expiry_i < int(now_epoch):
                return False

        # FIX PT-011: replay prevention with nonce set.
        nonces = seen_nonces if seen_nonces is not None else ACPMessage._seen_nonces
        nonce_key = f"{params.get('agent_id', '')}:{signed_epoch_i}:{nonce_s}"
        if nonce_key in nonces:
            return False
        nonces.add(nonce_key)
        return True


class RateLimiter:
    def __init__(self, max_per_epoch: int = 100, epoch_provider: Optional[Callable[[], int]] = None) -> None:
        self.max_per_epoch = max_per_epoch
        self.counts: Dict[Tuple[str, int], int] = {}
        self.epoch_provider = epoch_provider
        self._trusted_epoch = 0

    def set_epoch(self, epoch: int) -> None:
        self._trusted_epoch = int(epoch)

    def _current_epoch(self) -> int:
        if self.epoch_provider is not None:
            return int(self.epoch_provider())
        return self._trusted_epoch

    def allow(self, agent_id: str, epoch: Optional[int] = None) -> bool:
        # FIX PT-013: derive epoch from trusted internal clock and ignore caller spoofing.
        trusted_epoch = self._current_epoch()
        key = (agent_id, trusted_epoch)
        cnt = self.counts.get(key, 0)
        if cnt >= self.max_per_epoch:
            return False
        self.counts[key] = cnt + 1
        return True


# -----------------------------------------------------------------------------
# Optimization Tournament
# -----------------------------------------------------------------------------


@dataclass
class Proposal:
    agent_id: str
    epoch: int
    weights: List[float]
    mint_fee: float
    loss_estimate: float
    expected_return: float
    risk: float
    metadata: Dict[str, Any] = field(default_factory=dict)

    def commit_hash(self, secret: str) -> str:
        payload = json.dumps(
            {
                "agent_id": self.agent_id,
                "epoch": self.epoch,
                "weights": self.weights,
                "mint_fee": self.mint_fee,
                "loss_estimate": self.loss_estimate,
                "expected_return": self.expected_return,
                "risk": self.risk,
            },
            sort_keys=True,
        )
        return sha256_hex(payload + secret)


class OptimizationTournament:
    def __init__(
        self,
        registry: AgentRegistry,
        reputation: ReputationEngine,
        staking: Optional[StakingEconomics] = None,
        epoch_length: int = 3600,
        submission_ratio: float = 0.8,
        min_participants: int = 1,
        *,
        direct_submit_enabled: bool = True,
        max_mint_fee: float = 0.02,
    ) -> None:
        self.registry = registry
        self.reputation = reputation
        self.staking = staking
        self.epoch_length = epoch_length
        self.submission_ratio = submission_ratio
        self.min_participants = min_participants
        self.direct_submit_enabled = bool(direct_submit_enabled)
        self.max_mint_fee = max(0.0, float(max_mint_fee))

        self.current_epoch = 0
        self.tick = 0
        self.commitments: Dict[str, str] = {}
        self.commit_consumed: Set[str] = set()
        self.proposals: List[Proposal] = []
        self.proposal_agents: Set[str] = set()
        self.previous_winner: Optional[Proposal] = None
        self.current_params: Dict[str, Any] = {"weights": [0.25, 0.25, 0.25, 0.25], "mint_fee": 0.002}
        self.current_loss: Optional[float] = None
        self.treasury: float = 0.0

    def start_epoch(self, epoch: int) -> None:
        self.current_epoch = epoch
        self.tick = 0
        self.commitments.clear()
        self.commit_consumed.clear()
        self.proposals.clear()
        self.proposal_agents.clear()

    @property
    def submission_end_tick(self) -> int:
        return int(self.epoch_length * self.submission_ratio)

    def advance_tick(self, ticks: int = 1) -> None:
        self.tick = min(self.epoch_length, self.tick + ticks)

    def commit(self, agent_id: str, proposal_hash: str) -> bool:
        if self.tick >= self.submission_end_tick:
            return False
        rec = self.registry.get_record(agent_id)
        if rec is None or rec.status != "Active":
            return False
        min_stake = self.registry.min_stake_by_type.get(rec.agent_type, 0.0)
        if rec.stake < min_stake:
            return False
        # FIX PT-017: reject commit overwrite if prior commit is still unconsumed.
        if agent_id in self.commitments and agent_id not in self.commit_consumed:
            return False
        self.commitments[agent_id] = proposal_hash
        self.commit_consumed.discard(agent_id)
        return True

    def _accept_proposal_once(self, proposal: Proposal) -> bool:
        # FIX PT-007: enforce one valid proposal per agent per epoch.
        if proposal.agent_id in self.proposal_agents:
            return False
        self.proposals.append(proposal)
        self.proposal_agents.add(proposal.agent_id)
        return True

    def reveal(self, proposal: Proposal, secret: str) -> bool:
        if proposal.epoch != self.current_epoch:
            return False
        if self.tick < self.submission_end_tick:
            return False
        rec = self.registry.get_record(proposal.agent_id)
        if rec is None or rec.status != "Active":
            return False
        commit_hash = proposal.commit_hash(secret)
        if self.commitments.get(proposal.agent_id) != commit_hash:
            return False
        if rec.stake < self.registry.min_stake_by_type.get(rec.agent_type, 0.0):
            return False
        if not self._accept_proposal_once(proposal):
            return False
        # FIX PT-003: consume commit after successful reveal (one-time reveal).
        self.commit_consumed.add(proposal.agent_id)
        self.commitments.pop(proposal.agent_id, None)
        rec.proposals_submitted += 1
        return True

    def submit_direct(self, proposal: Proposal) -> bool:
        # FIX PT-004: allow operators to disable direct path in production mode.
        if not self.direct_submit_enabled:
            return False
        if proposal.epoch != self.current_epoch:
            return False
        rec = self.registry.get_record(proposal.agent_id)
        if rec is None or rec.status != "Active":
            return False
        if rec.stake < self.registry.min_stake_by_type.get(rec.agent_type, 0.0):
            return False
        if not self._accept_proposal_once(proposal):
            return False
        rec.proposals_submitted += 1
        return True

    def _score(self, proposal: Proposal) -> float:
        loss_score = -proposal.loss_estimate
        # FIX PT-005: clamp self-reported return/risk terms to bounded ranges.
        safe_return = max(-1.0, min(1.0, float(proposal.expected_return)))
        safe_risk = max(0.01, min(5.0, float(proposal.risk)))
        risk_adj = safe_return / safe_risk
        risk_adj *= 0.1
        rep = self.reputation.get(proposal.agent_id)
        rep_weight = 0.001 * rep
        novelty = 0.0
        copycat_penalty = 0.0
        if self.previous_winner is not None:
            sim = safe_cosine_similarity(proposal.weights, self.previous_winner.weights)
            novelty = (1.0 - sim) * 0.1
            if sim > 0.95:
                copycat_penalty = 0.2
        return loss_score + risk_adj + rep_weight + novelty - copycat_penalty

    def _validate_winner_params(self, proposal: Proposal) -> bool:
        weights = [float(w) for w in proposal.weights]
        if len(weights) != len(self.current_params.get("weights", [])):
            return False
        if any((w < 0.0 or w > 1.0) for w in weights):
            return False
        if abs(sum(weights) - 1.0) > 1e-6:
            return False
        fee = float(proposal.mint_fee)
        if fee < 0.0 or fee > self.max_mint_fee:
            return False
        return True

    def evaluate(self, epoch_fees: float) -> Optional[Proposal]:
        if len(self.proposals) < self.min_participants:
            return None
        ranked = sorted(self.proposals, key=self._score, reverse=True)

        winner: Optional[Proposal] = None
        for candidate in ranked:
            # FIX PT-006: enforce hard invariants before adoption.
            if self._validate_winner_params(candidate):
                winner = candidate
                break
        if winner is None:
            return None

        # If all proposals worse than current loss, keep current
        if self.current_loss is not None:
            best_loss = min(p.loss_estimate for p in self.proposals)
            if best_loss > self.current_loss * 1.05:
                return None

        self.previous_winner = winner
        self.current_params = {"weights": winner.weights, "mint_fee": winner.mint_fee}
        self.current_loss = winner.loss_estimate

        treasury_share = epoch_fees * 0.55
        winner_share = epoch_fees * 0.30
        runner_share = epoch_fees * 0.10 if len(ranked) > 1 else 0.0
        participant_pool = epoch_fees * 0.05
        self.treasury += treasury_share

        if self.staking:
            self.staking.reward(winner.agent_id, winner_share, self.current_epoch)
            runner = next((p for p in ranked if p.agent_id != winner.agent_id), None)
            if runner_share > 0.0 and runner is not None:
                self.staking.reward(runner.agent_id, runner_share, self.current_epoch)

            # FIX PT-007: split participant pool by unique agent count.
            if participant_pool > 0 and self.proposal_agents:
                per = participant_pool / len(self.proposal_agents)
                for agent_id in self.proposal_agents:
                    self.staking.reward(agent_id, per, self.current_epoch)

        self.reputation.add(winner.agent_id, 10)
        rec = self.registry.get_record(winner.agent_id)
        if rec:
            rec.proposals_accepted += 1
        return winner


# -----------------------------------------------------------------------------
# Federated Watchdog
# -----------------------------------------------------------------------------


class FederatedWatchdog:
    def __init__(
        self,
        registry: AgentRegistry,
        staking: StakingEconomics,
        reputation: ReputationEngine,
        max_evidence_age: int = 10,
        max_future_drift: int = 1,
    ) -> None:
        self.registry = registry
        self.staking = staking
        self.reputation = reputation
        self.max_evidence_age = max_evidence_age
        self.max_future_drift = max(0, int(max_future_drift))
        self.alerts: Dict[Tuple[int, str], Dict[str, Dict[str, Any]]] = {}
        self.methods: Dict[Tuple[int, str], Dict[str, str]] = {}
        self.report_order: Dict[Tuple[int, str], Dict[str, int]] = {}
        self.finalized: Set[Tuple[int, str]] = set()
        self.false_positive: Dict[str, int] = {}
        self.true_positive: Dict[str, int] = {}
        self._arrival_seq = 0

    def _active_monitors(self) -> List[AgentRecord]:
        return self.registry.active_agents("Monitor")

    def report(self, agent_id: str, alert_type: str, evidence: Dict[str, Any], epoch: int, method: str) -> bool:
        rec = self.registry.get_record(agent_id)
        if rec is None or rec.agent_type != "Monitor" or rec.status != "Active":
            return False
        if not evidence or "snapshot" not in evidence or "oracle" not in evidence or "timestamp" not in evidence:
            return False

        ts = int(evidence["timestamp"])
        if epoch - ts > self.max_evidence_age:
            return False
        # FIX PT-009: reject future timestamp beyond bounded drift.
        if ts > epoch + self.max_future_drift:
            return False

        key = (epoch, alert_type)
        if key in self.finalized:
            return False

        self.alerts.setdefault(key, {})[agent_id] = evidence
        self.methods.setdefault(key, {})[agent_id] = method
        self.report_order.setdefault(key, {})
        if agent_id not in self.report_order[key]:
            # FIX PT-010: preserve arrival order for first-reporter bounty.
            self._arrival_seq += 1
            self.report_order[key][agent_id] = self._arrival_seq
        return True

    def consensus(self, alert_type: str, epoch: int) -> bool:
        monitors = self._active_monitors()
        n = len(monitors)
        if n == 0:
            return False
        m = min(3, math.ceil(n / 2))
        votes = len(self.alerts.get((epoch, alert_type), {}))
        return votes >= m

    def fallback_required(self) -> bool:
        return len(self._active_monitors()) == 0

    def resolve(self, alert_type: str, epoch: int, is_true: bool) -> None:
        key = (epoch, alert_type)
        reports = self.alerts.get(key, {})
        if not reports:
            return
        # FIX PT-008: block duplicate resolve on finalized alerts.
        if key in self.finalized:
            return

        if is_true:
            order = self.report_order.get(key, {})
            if order:
                first = min(order.items(), key=lambda kv: kv[1])[0]
            else:
                first = next(iter(reports.keys()))
            self.staking.reward(first, 1.0, epoch)
            self.reputation.add(first, 20)
            self.true_positive[first] = self.true_positive.get(first, 0) + 1
        else:
            for agent_id in reports.keys():
                self.staking.slash(agent_id, 0.05, epoch)
                self.reputation.add(agent_id, -25)
                self.false_positive[agent_id] = self.false_positive.get(agent_id, 0) + 1

        self.finalized.add(key)

    def diversity_score(self, alert_type: str, epoch: int) -> float:
        methods = self.methods.get((epoch, alert_type), {})
        if not methods:
            return 0.0
        unique = len(set(methods.values()))
        return unique / max(len(methods), 1)


# -----------------------------------------------------------------------------
# Security Engine
# -----------------------------------------------------------------------------


def validate_oracle_data(oracle_payload: Dict[str, Any], min_sources: int = 2) -> bool:
    sources = oracle_payload.get("sources")
    if not isinstance(sources, list):
        return False
    return len(sources) >= min_sources


def enforce_monotonic_time(prev_epoch: int, new_epoch: int) -> bool:
    return new_epoch >= prev_epoch


class SecurityEngine:
    def __init__(self, registry: AgentRegistry) -> None:
        self.registry = registry

    def detect_sybil(self, min_cluster: int = 5) -> List[str]:
        clusters: Dict[str, List[str]] = {}
        for agent_id, meta in self.registry.meta.items():
            owner = meta.get("owner", "")
            if owner:
                clusters.setdefault(owner, []).append(agent_id)
        sybils = []
        for owner, agents in clusters.items():
            if len(agents) >= min_cluster:
                sybils.extend(agents)
        return sybils

    def detect_collusion(self, proposals: List[Proposal], threshold: float = 0.98) -> List[Tuple[str, str]]:
        colluded = []
        for i in range(len(proposals)):
            for j in range(i + 1, len(proposals)):
                sim = safe_cosine_similarity(proposals[i].weights, proposals[j].weights)
                if sim >= threshold:
                    colluded.append((proposals[i].agent_id, proposals[j].agent_id))
        return colluded

    def state_hash(self, state: Dict[str, Any]) -> str:
        return sha256_hex(json.dumps(state, sort_keys=True))


# -----------------------------------------------------------------------------
# Open Agent Simulation (orchestrator)
# -----------------------------------------------------------------------------


@dataclass
class AgentProfile:
    agent_id: str
    agent_type: str
    behavior: str
    owner: str
    methodology: str


class OpenAgentSimulation:
    def __init__(self, seed: int = 0, scenario: str = "normal") -> None:
        self.rng = random.Random(seed)
        self.scenario = scenario
        self.registry = AgentRegistry()
        self.reputation = ReputationEngine()
        self.staking = StakingEconomics(self.registry)
        self.tournament = OptimizationTournament(self.registry, self.reputation, self.staking)
        self.watchdog = FederatedWatchdog(self.registry, self.staking, self.reputation)
        self.security = SecurityEngine(self.registry)
        self.agents: List[AgentProfile] = []
        self.metrics: Dict[str, List[float]] = {"peg_error": [], "fees": []}
        self.safe_mode: bool = False

    def add_agent(self, agent_id: str, agent_type: str, behavior: str = "honest", owner: str = "", methodology: str = "default") -> None:
        stake = self.registry.min_stake_by_type.get(agent_type, 0.0)
        self.registry.register(agent_id, agent_type, stake, 0)
        self.staking.deposit(agent_id, agent_type, stake, 0)
        self.registry.set_meta(agent_id, owner=owner)
        self.agents.append(AgentProfile(agent_id, agent_type, behavior, owner, methodology))

    def setup_agents(self, num_optimizers: int, num_monitors: int, num_auditors: int = 0, num_liquidators: int = 0) -> None:
        for i in range(num_optimizers):
            behavior = "honest" if i % 2 == 0 else "lazy"
            self.add_agent(f"opt{i}", "Optimizer", behavior=behavior, owner=f"owner{i}")
        for i in range(num_monitors):
            self.add_agent(f"mon{i}", "Monitor", behavior="honest", owner=f"ownerM{i}", methodology=f"m{i}")
        for i in range(num_auditors):
            self.add_agent(f"aud{i}", "Auditor", behavior="honest", owner=f"ownerA{i}")
        for i in range(num_liquidators):
            self.add_agent(f"liq{i}", "Liquidator", behavior="honest", owner=f"ownerL{i}")

    def _market_params(self) -> Tuple[float, float]:
        if self.scenario == "normal":
            return 0.001, 0.002
        if self.scenario == "volatile":
            return 0.002, 0.01
        if self.scenario == "crash":
            return 0.005, 0.03
        if self.scenario == "recovery":
            return 0.0005, 0.002
        return 0.001, 0.005

    def _generate_peg_error(self) -> float:
        mu, sigma = self._market_params()
        return self.rng.gauss(mu, sigma)

    def _optimal_weights(self) -> List[float]:
        return [0.4, 0.3, 0.2, 0.1]

    def _proposal_for_agent(self, agent: AgentProfile, epoch: int) -> Optional[Proposal]:
        if agent.agent_type != "Optimizer":
            return None
        if agent.behavior == "lazy":
            return None
        optimal = self._optimal_weights()
        if agent.behavior == "malicious":
            weights = [1.0, 0.0, 0.0, 0.0]
            loss = 0.1
            expected_return = -0.02
            risk = 0.3
        else:
            noise = [self.rng.uniform(-0.005, 0.005) for _ in optimal]
            weights = [max(0.0, min(1.0, w + n)) for w, n in zip(optimal, noise)]
            s = sum(weights) or 1.0
            weights = [w / s for w in weights]
            loss = 0.005 + abs(self._generate_peg_error())
            expected_return = 0.02
            risk = 0.02
        return Proposal(
            agent_id=agent.agent_id,
            epoch=epoch,
            weights=weights,
            mint_fee=0.002,
            loss_estimate=loss,
            expected_return=expected_return,
            risk=risk,
        )

    def run_epoch(self, epoch: int) -> None:
        self.tournament.start_epoch(epoch)
        peg_error = self._generate_peg_error()
        self.metrics["peg_error"].append(abs(peg_error))
        proposals: Dict[str, Proposal] = {}
        # Commit phase
        for agent in self.agents:
            proposal = self._proposal_for_agent(agent, epoch)
            if proposal:
                proposals[agent.agent_id] = proposal
                secret = f"secret-{agent.agent_id}"
                self.tournament.commit(agent.agent_id, proposal.commit_hash(secret))
        # Reveal phase
        self.tournament.advance_tick(self.tournament.submission_end_tick)
        for agent_id, proposal in proposals.items():
            secret = f"secret-{agent_id}"
            self.tournament.reveal(proposal, secret)
        # Evaluate
        fees = 100.0
        winner = self.tournament.evaluate(fees)
        # Slash any clearly bad proposals
        behavior_by_id = {a.agent_id: a.behavior for a in self.agents}
        for prop in list(self.tournament.proposals):
            if prop.loss_estimate > 0.05:
                slash_ratio = 0.95 if behavior_by_id.get(prop.agent_id) == "malicious" else 0.10
                self.staking.slash(prop.agent_id, slash_ratio, epoch)
                self.reputation.add(prop.agent_id, -15)
        # Watchdog
        if abs(peg_error) > 0.02:
            evidence = {"snapshot": {"peg": peg_error}, "oracle": {"price": 1 + peg_error}, "timestamp": epoch}
            for agent in self.agents:
                if agent.agent_type == "Monitor":
                    self.watchdog.report(agent.agent_id, "PEG_DEVIATION", evidence, epoch, agent.methodology)
            if self.watchdog.consensus("PEG_DEVIATION", epoch):
                self.watchdog.resolve("PEG_DEVIATION", epoch, is_true=True)
        if self.watchdog.fallback_required():
            self.safe_mode = True

    def run(self, epochs: int = 100) -> Dict[str, Any]:
        for epoch in range(epochs):
            self.run_epoch(epoch)
        mae = sum(self.metrics["peg_error"]) / max(len(self.metrics["peg_error"]), 1)
        return {
            "peg_mae": mae,
            "treasury": self.tournament.treasury,
            "safe_mode": self.safe_mode,
            "rewards": dict(self.staking.balances),
        }

    def monte_carlo_convergence(self, epochs: int = 1000, runs: int = 100) -> float:
        maes = []
        optimal = self._optimal_weights()
        for r in range(runs):
            self.rng.seed(r)
            errors = []
            for epoch in range(epochs):
                proposal = self._proposal_for_agent(AgentProfile("tmp", "Optimizer", "honest", "", ""), epoch)
                if proposal:
                    err = sum(abs(a - b) for a, b in zip(proposal.weights, optimal)) / len(optimal)
                    errors.append(err)
            maes.append(sum(errors) / max(len(errors), 1))
        return sum(maes) / max(len(maes), 1)


# -----------------------------------------------------------------------------
# Utilities for Monte Carlo stats
# -----------------------------------------------------------------------------


def mc_stats(values: List[float]) -> Dict[str, float]:
    if not values:
        return {"mean": 0.0, "median": 0.0, "p5": 0.0, "p95": 0.0, "worst": 0.0}
    return {
        "mean": sum(values) / len(values),
        "median": percentile(values, 0.5),
        "p5": percentile(values, 0.05),
        "p95": percentile(values, 0.95),
        "worst": max(values),
    }


def run_monte_carlo_suite(seed: int = 0, runs: int = 100) -> Dict[str, Dict[str, float]]:
    rng = random.Random(seed)
    mae = []
    for r in range(runs):
        sim = OpenAgentSimulation(seed=rng.randint(0, 10_000), scenario="normal")
        sim.setup_agents(5, 3)
        result = sim.run(epochs=50)
        mae.append(result["peg_mae"])
    return {"peg_mae": mc_stats(mae)}
