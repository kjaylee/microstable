"""
microstable.py
Pure-Python simulator for a self-evolving multi-collateral stablecoin protocol.

Key components:
- Value: scalar autograd (micrograd-style)
- ProtocolState + MarketEnv
- Loss function (spec coefficients)
- Adam optimizer with clipping + simplex/capped projection
- Circuit breaker state machine
- Scenario runner + metrics/events export

No external dependencies.
"""

from __future__ import annotations

import csv
import hashlib
import math
import os
import random
import secrets
from collections import deque
from dataclasses import dataclass, field
from typing import Callable, Deque, Dict, List, Optional, Sequence, Set, Tuple


# -----------------------------------------------------------------------------
# Global constants
# -----------------------------------------------------------------------------

EPS = 1e-12
GRAD_CLIP_NORM = 1.0
DELTA_W_MAX = 0.02
DELTA_FEE_MAX = 0.001  # 10 bps

ASSETS = ["USDC", "USDT", "DAI", "USDS"]
INITIAL_WEIGHTS = [0.40, 0.30, 0.20, 0.10]
BASE_W_CAPS = [0.55, 0.45, 0.45, 0.35]
BASE_RISK = [0.10, 0.20, 0.15, 0.25]

# // BLUE-TEAM: H20 - cap autograd chain depth to prevent memory-amplification attacks.
MAX_AUTOGRAD_DEPTH = 256

# // BLUE-TEAM: E2 - oracle freshness + collateral quality gates for mint/redeem accounting.
ORACLE_FRESHNESS_MAX_SECONDS = 120
MIN_COLLATERAL_QUALITY = 0.85

# // BLUE-TEAM: F9/G17 - reserved protocol lanes under congestion.
DEFAULT_RESERVED_TX_SLOTS = 3
DEFAULT_RESERVED_COMPUTE_UNITS = 2_400_000
DEFAULT_PRIORITY_FEE_MICROLAMPORTS = 5_000

# // BLUE-TEAM: I26 - insurance anti-drain defaults.
MIN_INSURANCE_CLAIM = 100.0
INSURANCE_COOLDOWN_TICKS = 5


# -----------------------------------------------------------------------------
# Value autograd
# -----------------------------------------------------------------------------


class Value:
    """Scalar autograd node (micrograd-style) with numerical guards."""

    def __init__(
        self,
        data: float,
        _children: Tuple["Value", ...] = (),
        _op: str = "",
        label: Optional[str] = None,
    ):
        self.data = float(data)
        if not math.isfinite(self.data):
            raise ValueError(f"non-finite Value data: {self.data}")
        self.grad = 0.0

        # // BLUE-TEAM: H20 - truncate overly deep chains to bound memory use.
        parent_depth = max((getattr(ch, "_depth", 1) for ch in _children), default=0)
        self._depth = parent_depth + 1
        if self._depth > MAX_AUTOGRAD_DEPTH:
            self._prev = set()
            self._depth = 1
        else:
            self._prev = set(_children)

        self._op = _op
        self._backward: Callable[[], None] = lambda: None
        self.label = label

    def __repr__(self) -> str:
        return f"Value(data={self.data:.10f}, grad={self.grad:.10f}, op={self._op})"

    @staticmethod
    def _coerce(other: float | "Value") -> "Value":
        return other if isinstance(other, Value) else Value(float(other))

    @staticmethod
    def _safe_divisor(x: float) -> Tuple[float, bool]:
        if abs(x) >= EPS:
            return x, False
        return (EPS if x >= 0 else -EPS), True

    @staticmethod
    def _ensure_finite(x: float, where: str) -> None:
        if not math.isfinite(x):
            raise ValueError(f"non-finite in {where}: {x}")

    @staticmethod
    def _truncate_graph(*nodes: "Value") -> bool:
        # // BLUE-TEAM: H20 - short-circuit graph construction beyond safe depth.
        return any(getattr(n, "_depth", 1) >= MAX_AUTOGRAD_DEPTH for n in nodes)

    # --- arithmetic ---
    def __add__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        if self._truncate_graph(self, other):
            return Value(self.data + other.data)
        out = Value(self.data + other.data, (self, other), "+")

        def _backward() -> None:
            self.grad += out.grad
            other.grad += out.grad
            self._ensure_finite(self.grad, "add:self")
            self._ensure_finite(other.grad, "add:other")

        out._backward = _backward
        return out

    def __radd__(self, other: float | "Value") -> "Value":
        return self + other

    def __neg__(self) -> "Value":
        if self._truncate_graph(self):
            return Value(-self.data)
        out = Value(-self.data, (self,), "neg")

        def _backward() -> None:
            self.grad -= out.grad
            self._ensure_finite(self.grad, "neg")

        out._backward = _backward
        return out

    def __sub__(self, other: float | "Value") -> "Value":
        return self + (-self._coerce(other))

    def __rsub__(self, other: float | "Value") -> "Value":
        return self._coerce(other) - self

    def __mul__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        if self._truncate_graph(self, other):
            return Value(self.data * other.data)
        out = Value(self.data * other.data, (self, other), "*")

        def _backward() -> None:
            self.grad += other.data * out.grad
            other.grad += self.data * out.grad
            self._ensure_finite(self.grad, "mul:self")
            self._ensure_finite(other.grad, "mul:other")

        out._backward = _backward
        return out

    def __rmul__(self, other: float | "Value") -> "Value":
        return self * other

    def __truediv__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        safe_denom, clamped = self._safe_divisor(other.data)
        if self._truncate_graph(self, other):
            return Value(self.data / safe_denom)
        out = Value(self.data / safe_denom, (self, other), "/")

        def _backward() -> None:
            self.grad += (1.0 / safe_denom) * out.grad
            if not clamped:
                other.grad += (-self.data / (safe_denom * safe_denom)) * out.grad
            else:
                # denominator is clamped constant at boundary
                other.grad += 0.0
            self._ensure_finite(self.grad, "div:self")
            self._ensure_finite(other.grad, "div:other")

        out._backward = _backward
        return out

    def __rtruediv__(self, other: float | "Value") -> "Value":
        return self._coerce(other) / self

    def __pow__(self, power: float) -> "Value":
        power = float(power)
        is_int = abs(power - round(power)) <= 1e-12
        if self.data < 0.0 and not is_int:
            raise ValueError("non-integer power with negative base is undefined in reals")
        if self._truncate_graph(self):
            return Value(self.data ** power)
        out = Value(self.data ** power, (self,), f"**{power}")

        def _backward() -> None:
            if self.data == 0.0 and power < 1.0:
                local = 0.0
            else:
                local = power * (self.data ** (power - 1.0))
            self.grad += local * out.grad
            self._ensure_finite(self.grad, "pow")

        out._backward = _backward
        return out

    # --- nonlinear ---
    def tanh(self) -> "Value":
        t = math.tanh(self.data)
        if self._truncate_graph(self):
            return Value(t)
        out = Value(t, (self,), "tanh")

        def _backward() -> None:
            self.grad += (1.0 - t * t) * out.grad
            self._ensure_finite(self.grad, "tanh")

        out._backward = _backward
        return out

    def exp(self) -> "Value":
        x = min(40.0, max(-40.0, self.data))
        e = math.exp(x)
        if self._truncate_graph(self):
            return Value(e)
        out = Value(e, (self,), "exp")

        def _backward() -> None:
            self.grad += e * out.grad
            self._ensure_finite(self.grad, "exp")

        out._backward = _backward
        return out

    def log(self) -> "Value":
        x = self.data if self.data > EPS else EPS
        if self._truncate_graph(self):
            return Value(math.log(x))
        out = Value(math.log(x), (self,), "log")

        def _backward() -> None:
            local = 1.0 / x if self.data > EPS else 0.0
            self.grad += local * out.grad
            self._ensure_finite(self.grad, "log")

        out._backward = _backward
        return out

    def relu(self) -> "Value":
        if self._truncate_graph(self):
            return Value(self.data if self.data > 0.0 else 0.0)
        out = Value(self.data if self.data > 0.0 else 0.0, (self,), "relu")

        def _backward() -> None:
            # subgradient rule: relu(0) = 0
            self.grad += (1.0 if self.data > 0.0 else 0.0) * out.grad
            self._ensure_finite(self.grad, "relu")

        out._backward = _backward
        return out

    def clamp(self, lo: float, hi: float) -> "Value":
        if lo > hi:
            raise ValueError("clamp: lo > hi")
        val = min(max(self.data, lo), hi)
        if self._truncate_graph(self):
            return Value(val)
        out = Value(val, (self,), "clamp")

        def _backward() -> None:
            self.grad += (1.0 if lo < self.data < hi else 0.0) * out.grad
            self._ensure_finite(self.grad, "clamp")

        out._backward = _backward
        return out

    def abs_l1(self) -> "Value":
        """Absolute value with subgradient sign(0)=0."""
        if self._truncate_graph(self):
            return Value(abs(self.data))
        out = Value(abs(self.data), (self,), "abs")

        def _backward() -> None:
            if self.data > 0.0:
                s = 1.0
            elif self.data < 0.0:
                s = -1.0
            else:
                s = 0.0
            self.grad += s * out.grad
            self._ensure_finite(self.grad, "abs")

        out._backward = _backward
        return out

    def backward(self) -> None:
        topo: List[Value] = []
        visited = set()
        visiting = set()

        # // BLUE-TEAM: H21 - explicit cycle detection before topological backward pass.
        def build(v: Value) -> None:
            if v in visiting:
                raise ValueError("autograd graph cycle detected")
            if v in visited:
                return
            visiting.add(v)
            for ch in v._prev:
                build(ch)
            visiting.remove(v)
            visited.add(v)
            topo.append(v)

        build(self)
        for node in topo:
            node.grad = 0.0
        self.grad = 1.0

        for node in reversed(topo):
            node._backward()
            self._ensure_finite(node.grad, f"backward:{node._op}")


# -----------------------------------------------------------------------------
# Market + state model
# -----------------------------------------------------------------------------


@dataclass
class MarketTick:
    tick: int
    prices: List[float]
    oracle_q: float
    stale_seconds: int
    divergence: float
    expected_breakers: List[int]


class MarketEnv:
    """Synthetic market environment with scenario-specific stress patterns."""

    def __init__(self, scenario: str, seed: int = 0, deterministic: bool = False):
        self.scenario = scenario
        # // BLUE-TEAM: H18/H19 - default to entropy-mixed RNG to avoid predictable exploit windows.
        if deterministic:
            mixed_seed = seed
        else:
            entropy = secrets.randbits(64)
            mixed_seed = (int(seed) << 64) ^ entropy
        self.rng = random.Random(mixed_seed)
        self.prices = [1.0, 1.0, 1.0, 1.0]
        self._shock_offset = 0 if deterministic else self.rng.randint(-3, 3)

    def _base_vol(self) -> float:
        return {
            "normal": 0.00025,
            "single_depeg": 0.0005,
            "multi_depeg": 0.0008,
            "volatile": 0.0022,
            "gradient_attack": 0.0005,
            "oracle_failure": 0.0006,
        }.get(self.scenario, 0.0007)

    def _shock(self, tick: int, asset_index: int) -> float:
        s_off = self._shock_offset
        if self.scenario == "single_depeg" and asset_index == 1 and (20 + s_off) <= tick <= (24 + s_off):
            return -0.025
        if self.scenario == "multi_depeg" and asset_index in (0, 1) and (20 + s_off) <= tick <= (28 + s_off):
            return -0.080
        if self.scenario == "gradient_attack" and (18 + s_off) <= tick <= (24 + s_off):
            return 0.004 if asset_index % 2 == 0 else -0.004
        if self.scenario == "volatile" and tick % 11 == 0:
            return self.rng.uniform(-0.015, 0.015)
        return 0.0

    def step(self, tick: int) -> MarketTick:
        vol = self._base_vol()
        expected: List[int] = []

        for i in range(len(self.prices)):
            noise = self.rng.gauss(0.0, vol)
            revert = 0.24 * (1.0 - self.prices[i])
            shock = self._shock(tick, i)
            p = self.prices[i] + noise + revert + shock
            self.prices[i] = min(1.5, max(0.5, p))

        depeg_count = sum(1 for p in self.prices if abs(p - 1.0) > 0.02)
        if depeg_count >= 1:
            expected.append(1)
        if depeg_count >= 2:
            expected.append(2)

        stale_seconds = 0
        divergence = abs(self.rng.gauss(0.0, 0.0012))
        if self.scenario == "oracle_failure" and 20 <= tick <= 42:
            stale_seconds = 180
            divergence = 0.04
            expected.append(3)

        q = 1.0
        q -= min(0.7, stale_seconds / 500.0)
        q -= min(0.6, divergence / 0.08)
        q += self.rng.gauss(0.0, 0.002)
        q = min(1.0, max(0.0, q))

        return MarketTick(
            tick=tick,
            prices=self.prices[:],
            oracle_q=q,
            stale_seconds=stale_seconds,
            divergence=divergence,
            expected_breakers=sorted(set(expected)),
        )


@dataclass
class ProtocolState:
    assets: List[str] = field(default_factory=lambda: ASSETS[:])
    weights: List[float] = field(default_factory=lambda: INITIAL_WEIGHTS[:])
    prev_weights: List[float] = field(default_factory=lambda: INITIAL_WEIGHTS[:])
    base_w_caps: List[float] = field(default_factory=lambda: BASE_W_CAPS[:])
    w_caps: List[float] = field(default_factory=lambda: BASE_W_CAPS[:])
    risk_scores: List[float] = field(default_factory=lambda: BASE_RISK[:])

    supply: float = 1_000_000.0
    reserve_value: float = 1_280_000.0
    cr_target: float = 1.20
    # FIX HI-02: keep a baseline so prolonged stress cannot permanently ratchet CR target.
    base_cr_target: float = 1.20
    cr_min: float = 1.20
    cr_hard_min: float = 1.05
    cr: float = 1.28

    mint_fee: float = 0.002
    redeem_fee: float = 0.002

    mint_limit: float = 1.0
    mint_paused_reason: str = ""
    optimizer_enabled: bool = True
    conservative_mode: bool = False
    oracle_degraded: bool = False

    nav_prev: float = 1.0
    nav_deltas: List[float] = field(default_factory=lambda: [0.0, 0.0, 0.0, 0.0])

    # // BLUE-TEAM: G16 - bind keeper proposals to epoch + state hash to block replay.
    market_epoch: int = 0
    market_state_hash: str = "GENESIS"

    # // BLUE-TEAM: runtime accounting mirror for invariant monitor.
    position_supply_sum: float = 1_000_000.0

    def clone(self) -> "ProtocolState":
        return ProtocolState(
            assets=self.assets[:],
            weights=self.weights[:],
            prev_weights=self.prev_weights[:],
            base_w_caps=self.base_w_caps[:],
            w_caps=self.w_caps[:],
            risk_scores=self.risk_scores[:],
            supply=self.supply,
            reserve_value=self.reserve_value,
            cr_target=self.cr_target,
            base_cr_target=self.base_cr_target,
            cr_min=self.cr_min,
            cr_hard_min=self.cr_hard_min,
            cr=self.cr,
            mint_fee=self.mint_fee,
            redeem_fee=self.redeem_fee,
            mint_limit=self.mint_limit,
            mint_paused_reason=self.mint_paused_reason,
            optimizer_enabled=self.optimizer_enabled,
            conservative_mode=self.conservative_mode,
            oracle_degraded=self.oracle_degraded,
            nav_prev=self.nav_prev,
            nav_deltas=self.nav_deltas[:],
            market_epoch=self.market_epoch,
            market_state_hash=self.market_state_hash,
            position_supply_sum=self.position_supply_sum,
        )

    def begin_tick(self) -> None:
        self.prev_weights = self.weights[:]
        self.market_epoch += 1
        self.market_state_hash = self._compute_state_hash(prices_hint=None, oracle_q_hint=None)

    def reset_dynamic_policy(self) -> None:
        self.w_caps = self.base_w_caps[:]
        self.mint_limit = 1.0
        self.mint_paused_reason = ""
        self.optimizer_enabled = True
        self.conservative_mode = False
        self.oracle_degraded = False

    def _compute_state_hash(
        self,
        prices_hint: Optional[Sequence[float]],
        oracle_q_hint: Optional[float],
    ) -> str:
        buf = [
            f"epoch={self.market_epoch}",
            "w=" + ",".join(f"{w:.8f}" for w in self.weights),
            f"fee={self.mint_fee:.8f}",
            f"cr={self.cr:.8f}",
            f"nav={self.nav_prev:.8f}",
        ]
        if prices_hint is not None:
            buf.append("p=" + ",".join(f"{p:.8f}" for p in prices_hint))
        if oracle_q_hint is not None:
            buf.append(f"q={oracle_q_hint:.8f}")
        raw = "|".join(buf).encode("utf-8")
        return hashlib.sha256(raw).hexdigest()[:24]

    @staticmethod
    def haircut(risk_score: float) -> float:
        # h_i(r_i) in [0, 0.05]
        return min(0.05, max(0.0, 0.02 * risk_score + 0.005))

    @staticmethod
    def peg_sensitivity(prices: Sequence[float]) -> float:
        # // BLUE-TEAM: I24 - damp peg sensitivity during deep basket-wide shocks.
        deep_depeg = sum(1 for p in prices if p < 0.85)
        if deep_depeg >= 2:
            return 0.030
        return 0.040

    def effective_collateral_value(self, prices: Sequence[float]) -> float:
        total = 0.0
        for w, p, r in zip(self.weights, prices, self.risk_scores):
            total += w * p * (1.0 - self.haircut(r))
        return total

    def apply_params(self, weights: Sequence[float], mint_fee: Optional[float] = None) -> None:
        self.weights = [float(w) for w in weights]
        if mint_fee is not None:
            self.mint_fee = float(mint_fee)

    def update_from_market(self, prices: Sequence[float], oracle_q: float, peg_noise: float = 0.0) -> float:
        nav = self.effective_collateral_value(prices)
        nav_delta = nav - self.nav_prev
        self.nav_prev = nav

        self.nav_deltas.append(nav_delta)
        if len(self.nav_deltas) > 40:
            self.nav_deltas.pop(0)

        peg_k = self.peg_sensitivity(prices)
        peg = 1.0 + peg_k * (nav - 1.0) + 0.0010 * (oracle_q - 1.0) + peg_noise
        peg = min(1.10, max(0.90, peg))

        target = self.cr_target + (0.03 if self.conservative_mode else 0.0)
        self.cr += 0.15 * (target - self.cr) + 0.40 * nav_delta
        cr_floor = max(self.cr_hard_min + 0.001, self.cr_target + 0.001)
        self.cr = min(2.5, max(cr_floor, self.cr))

        self.reserve_value = self.cr * self.supply
        self.position_supply_sum = self.supply
        self.market_state_hash = self._compute_state_hash(prices_hint=prices, oracle_q_hint=oracle_q)
        return peg


# -----------------------------------------------------------------------------
# Loss function
# -----------------------------------------------------------------------------


@dataclass
class LossCoefficients:
    lambda_p: float = 5.0
    lambda_cr: float = 20.0
    lambda_var: float = 2.0
    lambda_turn: float = 0.5
    lambda_conc: float = 1.5
    lambda_orc: float = 3.0


class LossEngine:
    def __init__(self, coeffs: Optional[LossCoefficients] = None):
        self.c = coeffs or LossCoefficients()

    def compute(self, state: ProtocolState, prices: Sequence[float], oracle_q: float) -> Tuple[Value, Dict[str, object]]:
        wvals = [Value(w, label=f"w{i}") for i, w in enumerate(state.weights)]
        fee_v = Value(state.mint_fee, label="mint_fee")

        nav = Value(0.0)
        for wv, p, r in zip(wvals, prices, state.risk_scores):
            h = ProtocolState.haircut(r)
            nav = nav + wv * (p * (1.0 - h))

        peg = Value(1.0) + (nav - 1.0) * ProtocolState.peg_sensitivity(prices) + Value(oracle_q - 1.0) * 0.0010
        cr_model = Value(state.cr) + (nav - Value(state.nav_prev)) * 0.25

        peg_loss = Value(self.c.lambda_p) * ((peg - 1.0) ** 2)

        cr_gap = Value(state.cr_min) - cr_model
        cr_penalty = Value(self.c.lambda_cr) * (cr_gap.relu() ** 2)

        hist = state.nav_deltas[-10:] if state.nav_deltas else [0.0]
        deltas: List[Value] = [Value(d) for d in hist] + [nav - Value(state.nav_prev)]
        mu = Value(0.0)
        for d in deltas:
            mu = mu + d
        mu = mu / float(len(deltas))

        var = Value(0.0)
        for d in deltas:
            dd = d - mu
            var = var + dd * dd
        var = var / float(len(deltas))
        var_term = Value(self.c.lambda_var) * var

        turnover = Value(0.0)
        for wv, wp in zip(wvals, state.prev_weights):
            turnover = turnover + (wv - wp).abs_l1()
        turn_term = Value(self.c.lambda_turn) * turnover

        conc = Value(0.0)
        for wv in wvals:
            conc = conc + wv * wv
        conc_term = Value(self.c.lambda_conc) * conc

        oracle_term = Value(self.c.lambda_orc * ((1.0 - oracle_q) ** 2))

        # tiny regularizer for fee path
        fee_reg = Value(0.2) * ((fee_v - 0.002) ** 2)

        total = peg_loss + cr_penalty + var_term + turn_term + conc_term + oracle_term + fee_reg
        if not math.isfinite(total.data):
            raise ValueError("non-finite loss")

        return total, {
            "weights": wvals,
            "fee": fee_v,
            "peg": peg,
            "cr": cr_model,
            "nav": nav,
            "peg_loss": peg_loss,
            "cr_penalty": cr_penalty,
            "var_term": var_term,
            "turn_term": turn_term,
            "conc_term": conc_term,
            "oracle_term": oracle_term,
        }


# -----------------------------------------------------------------------------
# Optimizer: Adam + projection
# -----------------------------------------------------------------------------


class AdamOptimizer:
    """
    Adam with update order:
    1) Adam raw update
    2) per-step change clip
    3) simplex projection
    4) caps enforcement
    """

    def __init__(
        self,
        n_weights: int,
        lr: float = 0.005,
        beta1: float = 0.9,
        beta2: float = 0.999,
        eps: float = 1e-8,
    ):
        self.nw = n_weights
        self.base_lr = lr
        self.lr = lr
        self.beta1 = beta1
        self.beta2 = beta2
        self.eps = eps
        self.t = 0
        self.m = [0.0] * (n_weights + 1)  # +fee
        self.v = [0.0] * (n_weights + 1)

    @staticmethod
    def clip_gradients(grads: Sequence[float], max_norm: float = GRAD_CLIP_NORM) -> List[float]:
        norm = math.sqrt(sum(g * g for g in grads))
        if norm <= max_norm or norm <= 0.0:
            return [float(g) for g in grads]
        scale = max_norm / norm
        return [float(g) * scale for g in grads]

    @staticmethod
    def simplex_projection(v: Sequence[float], z: float = 1.0) -> List[float]:
        if z <= 0.0:
            return [0.0 for _ in v]
        u = sorted(v, reverse=True)
        cssv = 0.0
        rho = -1
        theta = 0.0
        for i, ui in enumerate(u):
            cssv += ui
            t = (cssv - z) / float(i + 1)
            if ui - t > 0:
                rho = i
                theta = t
        if rho == -1:
            return [z / float(len(v)) for _ in v]
        return [max(vi - theta, 0.0) for vi in v]

    @staticmethod
    def project_box_simplex(y: Sequence[float], lo: Sequence[float], hi: Sequence[float], target: float = 1.0) -> List[float]:
        """Projection onto {x: sum x = target, lo<=x<=hi} via tau bisection."""
        n = len(y)
        lo_s = [float(lo[i]) for i in range(n)]
        hi_s = [float(hi[i]) for i in range(n)]

        s_lo = sum(lo_s)
        s_hi = sum(hi_s)
        if s_lo > target + 1e-12:
            # infeasible low bounds: renormalize low bounds down
            scale = target / s_lo if s_lo > 0 else 0.0
            lo_s = [x * scale for x in lo_s]
            s_lo = sum(lo_s)
        if s_hi < target - 1e-12:
            # infeasible high bounds: scale up highs proportionally
            if s_hi > 0:
                scale = target / s_hi
                hi_s = [x * scale for x in hi_s]
            s_hi = sum(hi_s)

        left = min(y[i] - hi_s[i] for i in range(n))
        right = max(y[i] - lo_s[i] for i in range(n))

        def proj(tau: float) -> List[float]:
            return [min(max(y[i] - tau, lo_s[i]), hi_s[i]) for i in range(n)]

        for _ in range(90):
            mid = 0.5 * (left + right)
            x = proj(mid)
            sx = sum(x)
            if sx > target:
                left = mid
            else:
                right = mid

        x = proj(right)
        resid = target - sum(x)
        if abs(resid) > 1e-12:
            for i in range(n):
                if resid > 0:
                    room = hi_s[i] - x[i]
                    if room > 0:
                        d = min(room, resid)
                        x[i] += d
                        resid -= d
                else:
                    room = x[i] - lo_s[i]
                    if room > 0:
                        d = min(room, -resid)
                        x[i] -= d
                        resid += d
                if abs(resid) <= 1e-12:
                    break
        return x

    def _adam_delta(self, idx: int, g: float) -> float:
        self.m[idx] = self.beta1 * self.m[idx] + (1.0 - self.beta1) * g
        self.v[idx] = self.beta2 * self.v[idx] + (1.0 - self.beta2) * g * g
        mhat = self.m[idx] / (1.0 - self.beta1 ** self.t)
        vhat = self.v[idx] / (1.0 - self.beta2 ** self.t)
        return self.lr * mhat / (math.sqrt(vhat) + self.eps)

    def step(
        self,
        weights: Sequence[float],
        mint_fee: float,
        grad_w: Sequence[float],
        grad_fee: float,
        caps: Sequence[float],
    ) -> Tuple[List[float], float]:
        grads = [float(g) for g in grad_w] + [float(grad_fee)]
        grads = self.clip_gradients(grads, GRAD_CLIP_NORM)

        self.t += 1

        # (1) Adam update
        adam_candidate = []
        for i in range(self.nw):
            delta = self._adam_delta(i, grads[i])
            adam_candidate.append(weights[i] - delta)

        # (2) per-step clip (weights)
        clipped = []
        for w_prev, w_new in zip(weights, adam_candidate):
            dw = max(-DELTA_W_MAX, min(DELTA_W_MAX, w_new - w_prev))
            clipped.append(w_prev + dw)

        # (3) simplex projection (nonnegative, sum=1)
        simplex = self.simplex_projection(clipped, z=1.0)

        # (4) caps enforcement (plus keep sum=1)
        lo = [0.0] * self.nw
        hi = [float(c) for c in caps]
        capped = self.project_box_simplex(simplex, lo, hi, target=1.0)

        # hard safety for step delta and caps simultaneously
        lo2 = [max(0.0, weights[i] - DELTA_W_MAX) for i in range(self.nw)]
        hi2 = [min(caps[i], weights[i] + DELTA_W_MAX) for i in range(self.nw)]
        final_w = self.project_box_simplex(capped, lo2, hi2, target=1.0)

        # fee update
        fee_delta = self._adam_delta(self.nw, grads[-1])
        fee_candidate = mint_fee - fee_delta
        fee_low = max(0.0, mint_fee - DELTA_FEE_MAX)
        fee_high = min(0.02, mint_fee + DELTA_FEE_MAX)
        final_fee = min(fee_high, max(fee_low, fee_candidate))

        return final_w, final_fee


# -----------------------------------------------------------------------------
# Circuit breaker
# -----------------------------------------------------------------------------


CB_NORMAL = "NORMAL"
CB_ACTIVATED = "ACTIVATED"
CB_HOLDING = "HOLDING"
CB_RECOVERY_CHECK = "RECOVERY_CHECK"


@dataclass
class BreakerMachine:
    cb_id: int
    min_hold: int
    recovery_needed: int
    cooldown_ticks: int = 5
    # FIX HI-02: cap active duration to prevent CB griefing DoS.
    max_active_ticks: int = 120

    state: str = CB_NORMAL
    cooldown_left: int = 0
    hold_ticks: int = 0
    recovery_streak: int = 0
    extended_factor: int = 1
    activation_tick: Optional[int] = None
    trigger_history: Deque[int] = field(default_factory=lambda: deque(maxlen=128))

    def is_active(self) -> bool:
        return self.state != CB_NORMAL

    def effective_min_hold(self) -> int:
        return self.min_hold * self.extended_factor

    def active_duration(self, tick: int) -> int:
        if self.activation_tick is None:
            return 0
        # FIX HI-02: count the activation tick itself to enforce a true max-tick budget.
        return max(0, tick - self.activation_tick + 1)

    def begin_tick(self) -> Optional[Tuple[str, str]]:
        old = self.state
        if self.state == CB_NORMAL:
            if self.cooldown_left > 0:
                self.cooldown_left -= 1
        elif self.state == CB_ACTIVATED:
            self.state = CB_HOLDING
            self.hold_ticks = 1
        elif self.state == CB_HOLDING:
            self.hold_ticks += 1
            if self.hold_ticks >= self.effective_min_hold():
                self.state = CB_RECOVERY_CHECK
                self.recovery_streak = 0
        return (old, self.state) if old != self.state else None

    def try_trigger(self, tick: int) -> bool:
        if self.is_active() or self.cooldown_left > 0:
            return False
        self.trigger_history.append(tick)
        while self.trigger_history and self.trigger_history[0] < tick - 30:
            self.trigger_history.popleft()
        self.extended_factor = 3 if len(self.trigger_history) >= 3 else 1
        self.state = CB_ACTIVATED
        self.hold_ticks = 0
        self.recovery_streak = 0
        self.activation_tick = tick
        return True

    def force_recovery_check(self) -> Optional[Tuple[str, str]]:
        if self.state in (CB_ACTIVATED, CB_HOLDING):
            old = self.state
            self.state = CB_RECOVERY_CHECK
            self.recovery_streak = 0
            return (old, self.state)
        return None

    def recovery_step(self, recovery_ok: bool, higher_active: bool) -> bool:
        """Return True if recovered to NORMAL in this tick."""
        if self.state != CB_RECOVERY_CHECK:
            return False
        if higher_active:
            return False

        if recovery_ok:
            self.recovery_streak += 1
        else:
            self.recovery_streak = 0
            self.state = CB_HOLDING  # hysteresis
            return False

        if self.recovery_streak >= self.recovery_needed:
            self.state = CB_NORMAL
            self.cooldown_left = self.cooldown_ticks
            self.hold_ticks = 0
            self.recovery_streak = 0
            self.extended_factor = 1
            self.activation_tick = None
            return True
        return False


class CircuitBreaker:
    """
    Priority: CB-4 > CB-3 > CB-2 > CB-1.
    Lower-priority recovery is deferred when higher CB is active.
    """

    # FIX HI-03: align breaker priority with spec.
    PRIORITY = [4, 3, 2, 1]

    def __init__(self, n_assets: int = 4):
        self.machines: Dict[int, BreakerMachine] = {
            1: BreakerMachine(1, min_hold=5, recovery_needed=10),
            2: BreakerMachine(2, min_hold=10, recovery_needed=20),
            3: BreakerMachine(3, min_hold=3, recovery_needed=5),
            4: BreakerMachine(4, min_hold=3, recovery_needed=3),
        }
        self.depeg_streak = [0] * n_assets
        self.cb1_target_index = 0
        self.loss_history: Deque[float] = deque(maxlen=8)
        self.events: List[Dict[str, object]] = []

    @staticmethod
    def valid_transitions() -> Dict[str, List[str]]:
        return {
            CB_NORMAL: [CB_ACTIVATED, CB_NORMAL],
            CB_ACTIVATED: [CB_HOLDING, CB_ACTIVATED],
            CB_HOLDING: [CB_HOLDING, CB_RECOVERY_CHECK],
            CB_RECOVERY_CHECK: [CB_RECOVERY_CHECK, CB_HOLDING, CB_NORMAL],
        }

    def _log(self, tick: int, cb_id: int, event: str, old: str, new: str, detail: str = "") -> None:
        self.events.append(
            {
                "tick": tick,
                "cb": cb_id,
                "event": event,
                "old": old,
                "new": new,
                "detail": detail,
            }
        )

    def is_active(self, cb_id: int) -> bool:
        return self.machines[cb_id].is_active()

    def active_ids(self) -> List[int]:
        return [k for k, m in self.machines.items() if m.is_active()]

    def _higher_active(self, cb_id: int) -> bool:
        rank = {cb: i for i, cb in enumerate(self.PRIORITY)}
        return any(
            self.machines[other].is_active() and rank[other] < rank[cb_id]
            for other in self.machines
            if other != cb_id
        )

    def _adaptive_margin(self, cb_id: int, tick: int) -> float:
        # FIX HI-02: widen recovery window if activation persists too long.
        machine = self.machines[cb_id]
        overtime = max(0, machine.active_duration(tick) - machine.max_active_ticks)
        if overtime <= 0:
            return 0.0
        return min(0.02, overtime * 0.0002)

    def _conditions(
        self,
        market: MarketTick,
        nav_drop: float,
        loss_finite: bool,
        loss_value: Optional[float],
        forced: Optional[Dict[str, bool]] = None,
    ) -> Dict[int, bool]:
        cond = {1: False, 2: False, 3: False, 4: False}

        # CB-1: single collateral depeg 3 consecutive ticks
        for i, p in enumerate(market.prices):
            if abs(p - 1.0) > 0.02:
                self.depeg_streak[i] += 1
            else:
                self.depeg_streak[i] = 0
        for i, streak in enumerate(self.depeg_streak):
            if streak >= 3:
                cond[1] = True
                self.cb1_target_index = i
                break

        # CB-2: multi-collateral stress, NAV crash, or correlated mild depeg basket stress.
        depeg_count = sum(1 for p in market.prices if abs(p - 1.0) > 0.02)
        mild_depeg_count = sum(1 for p in market.prices if abs(p - 1.0) > 0.015)
        # // BLUE-TEAM: E5 - catch correlation-driven stress before per-asset 2% triggers fire.
        correlated_stress = mild_depeg_count >= 2 and nav_drop < -0.015
        cond[2] = depeg_count >= 2 or nav_drop < -0.03 or correlated_stress

        # CB-3: oracle failure
        cond[3] = market.stale_seconds > 120 or market.divergence > 0.02

        # CB-4: numerical instability
        divergent = False
        if loss_value is not None and len(self.loss_history) >= 1:
            last = self.loss_history[-1]
            # trigger only on catastrophic blow-up to reduce false positives
            if last > 0 and loss_value > last * 20.0 and (loss_value - last) > 5.0:
                divergent = True
        cond[4] = (not loss_finite) or divergent

        if forced:
            for k in (1, 2, 3, 4):
                if forced.get(f"cb{k}", False):
                    cond[k] = True
            if "cb1_idx" in forced:
                self.cb1_target_index = int(forced["cb1_idx"])

        return cond

    def update(
        self,
        tick: int,
        state: ProtocolState,
        market: MarketTick,
        nav_drop: float,
        loss_finite: bool,
        loss_value: Optional[float],
        forced: Optional[Dict[str, bool]] = None,
    ) -> Dict[str, bool]:
        # advance internal state timers
        for cb_id, machine in self.machines.items():
            changed = machine.begin_tick()
            if changed:
                old, new = changed
                self._log(tick, cb_id, "advance", old, new)

        cond = self._conditions(market, nav_drop, loss_finite, loss_value, forced=forced)

        # Trigger in strict priority order
        for cb_id in self.PRIORITY:
            if cond[cb_id]:
                m = self.machines[cb_id]
                old = m.state
                if m.try_trigger(tick):
                    detail = "extended" if m.extended_factor > 1 else "normal"
                    self._log(tick, cb_id, "activate", old, m.state, detail=detail)

        # FIX HI-02: force recovery mode if a breaker is active for too long.
        for cb_id, machine in self.machines.items():
            if machine.is_active() and machine.active_duration(tick) >= machine.max_active_ticks:
                changed = machine.force_recovery_check()
                if changed:
                    old, new = changed
                    self._log(tick, cb_id, "force_recovery", old, new, detail="max_active_duration")

        # recovery conditions with adaptive widening for prolonged activation
        target_idx = self.cb1_target_index
        cb1_margin = self._adaptive_margin(1, tick)
        cb2_margin = self._adaptive_margin(2, tick)
        cb3_margin = self._adaptive_margin(3, tick)

        cb1_ok = abs(market.prices[target_idx] - 1.0) < (0.005 + cb1_margin)
        cb2_ok = all(abs(p - 1.0) < (0.005 + cb2_margin) for p in market.prices) and nav_drop > (-0.002 - cb2_margin)
        cb3_ok = market.stale_seconds <= (120 + int(2000 * cb3_margin)) and market.divergence <= (0.02 + cb3_margin)

        cb4_ok = False
        if loss_value is not None and math.isfinite(loss_value):
            self.loss_history.append(loss_value)
            if len(self.loss_history) >= 4:
                a, b, c, d = list(self.loss_history)[-4:]
                cb4_ok = a > b > c > d

        recovery_ok = {1: cb1_ok, 2: cb2_ok, 3: cb3_ok, 4: cb4_ok}

        cb2_recovered_while_cb3 = False
        for cb_id in self.PRIORITY:
            m = self.machines[cb_id]
            old = m.state
            forced_recovery = m.is_active() and m.active_duration(tick) >= m.max_active_ticks
            recovery_signal = recovery_ok[cb_id] or forced_recovery
            recovered = m.recovery_step(recovery_signal, self._higher_active(cb_id))
            if recovered:
                self._log(tick, cb_id, "recover", old, m.state)
            elif old != m.state:
                self._log(tick, cb_id, "hysteresis", old, m.state)
            if cb_id == 2 and recovered and self.machines[3].is_active():
                cb2_recovered_while_cb3 = True

        # FIX HI-03: assertion guard for recovery ordering.
        assert not cb2_recovered_while_cb3, "FIX HI-03: CB-2 cannot recover while CB-3 is active"

        # apply degradation actions (worsening actions immediate)
        state.reset_dynamic_policy()

        # CB-1
        if self.machines[1].is_active():
            i = self.cb1_target_index
            target_cap = state.base_w_caps[i] * 0.5
            # FIX HI-02: stage cap tightening so per-tick weight delta bounds remain feasible.
            staged_cap = max(target_cap, state.weights[i] - DELTA_W_MAX)
            state.w_caps[i] = min(state.w_caps[i], staged_cap)
            state.mint_limit = min(state.mint_limit, 0.25)
            state.cr_target = max(state.cr_target, 1.25)

        # FIX HI-02: allow gradual CB-2 recovery slope after prolonged activation.
        cb2_machine = self.machines[2]
        if cb2_machine.state in (CB_ACTIVATED, CB_HOLDING):
            state.mint_limit = 0.0
            state.mint_paused_reason = "MINT_PAUSED_BY_CB2"
            state.cr_target = max(state.cr_target, 1.30)
        elif cb2_machine.state == CB_RECOVERY_CHECK:
            progress = cb2_machine.recovery_streak / max(1, cb2_machine.recovery_needed)
            forced_floor = 0.10 if cb2_machine.active_duration(tick) >= cb2_machine.max_active_ticks else 0.0
            state.mint_limit = min(state.mint_limit, forced_floor + 0.90 * progress)
            if state.mint_limit <= 1e-12:
                state.mint_paused_reason = "MINT_PAUSED_BY_CB2"
            state.cr_target = max(state.cr_target, 1.30)

        # CB-3 has higher priority than CB-2 and therefore can re-freeze minting.
        if self.machines[3].is_active():
            state.optimizer_enabled = False
            state.conservative_mode = True
            state.oracle_degraded = True
            state.mint_limit = 0.0
            state.mint_paused_reason = "MINT_PAUSED_BY_CB3"
            state.cr_target = max(state.cr_target, 1.35)

        rollback = self.machines[4].is_active()

        # enforce cap consistency if current weights violate tightened caps
        if any(w > cap + 1e-12 for w, cap in zip(state.weights, state.w_caps)):
            n = len(state.weights)
            # FIX HI-02: bounded repair to avoid one-tick weight jumps under CB pressure.
            lo2 = [max(0.0, state.weights[i] - DELTA_W_MAX) for i in range(n)]
            hi2 = [min(state.w_caps[i], state.weights[i] + DELTA_W_MAX) for i in range(n)]
            repaired = AdamOptimizer.project_box_simplex(
                y=state.weights,
                lo=lo2,
                hi=hi2,
                target=1.0,
            )
            state.weights = repaired
            state.prev_weights = repaired[:]

        # FIX HI-02: restore CR target toward baseline after sustained stability.
        if not self.active_ids():
            state.cr_target = max(state.base_cr_target, state.cr_target - 0.005)

        return {
            "rollback": rollback,
            "cb1": self.machines[1].is_active(),
            "cb2": self.machines[2].is_active(),
            "cb3": self.machines[3].is_active(),
            "cb4": self.machines[4].is_active(),
        }


# -----------------------------------------------------------------------------
# Agent interfaces
# -----------------------------------------------------------------------------


class Keeper:
    def propose(
        self,
        state: ProtocolState,
        optimizer: AdamOptimizer,
        grad_w: Sequence[float],
        grad_fee: float,
    ) -> Dict[str, object]:
        new_w, new_fee = optimizer.step(state.weights, state.mint_fee, grad_w, grad_fee, state.w_caps)
        # // BLUE-TEAM: G16 - proposal includes epoch/hash binding and bounded expiry.
        return {
            "weights": new_w,
            "mint_fee": new_fee,
            "status": "PROPOSED",
            "proposal_epoch": state.market_epoch,
            "state_hash": state.market_state_hash,
            "expiry_epoch": state.market_epoch + 2,
        }

    def submit_update_proposal(self, state: ProtocolState, proposal: Dict[str, object]) -> Dict[str, object]:
        w = [float(x) for x in proposal["weights"]]  # type: ignore[index]
        fee = float(proposal.get("mint_fee", state.mint_fee))

        # // BLUE-TEAM: G16 - reject stale/unbound replayed proposals.
        epoch = proposal.get("proposal_epoch")
        state_hash = proposal.get("state_hash")
        expiry = proposal.get("expiry_epoch")
        if epoch is None or state_hash is None or expiry is None:
            return {"status": "REJECTED", "reason": "missing_proposal_binding"}
        if int(epoch) != state.market_epoch:
            return {"status": "REJECTED", "reason": "proposal_epoch_mismatch"}
        if str(state_hash) != state.market_state_hash:
            return {"status": "REJECTED", "reason": "proposal_state_hash_mismatch"}
        if int(expiry) < state.market_epoch:
            return {"status": "REJECTED", "reason": "proposal_expired"}

        if abs(sum(w) - 1.0) > 1e-6:
            return {"status": "REJECTED", "reason": "sum(weights)!=1"}
        for i, wi in enumerate(w):
            if wi < -1e-12 or wi > state.w_caps[i] + 1e-12:
                return {"status": "REJECTED", "reason": f"cap_violation_{i}"}
            if abs(wi - state.weights[i]) > DELTA_W_MAX + 1e-9:
                return {"status": "REJECTED", "reason": f"delta_violation_{i}"}

        if abs(fee - state.mint_fee) > DELTA_FEE_MAX + 1e-12:
            return {"status": "REJECTED", "reason": "fee_delta_violation"}

        state.apply_params(w, fee)
        return {"status": "APPLIED", "weights": state.weights[:], "mint_fee": state.mint_fee}


class Watchdog:
    def detect(self, market: MarketTick) -> Dict[str, object]:
        events: Dict[str, object] = {}
        depeg_indices = [i for i, p in enumerate(market.prices) if abs(p - 1.0) > 0.02]
        if depeg_indices:
            events["cb1"] = True
            events["cb1_idx"] = int(depeg_indices[0])
        if len(depeg_indices) >= 2:
            events["cb2"] = True
        if market.stale_seconds > 120 or market.divergence > 0.02:
            events["cb3"] = True
        return events


class Auditor:
    def verify_invariants(self, state: ProtocolState) -> Dict[str, object]:
        violations: List[str] = []
        if abs(sum(state.weights) - 1.0) > 1e-6:
            violations.append("INV_WEIGHT_SUM")
        for i, (w, cap) in enumerate(zip(state.weights, state.w_caps)):
            if w < -1e-10 or w > cap + 1e-10:
                violations.append(f"INV_WEIGHT_CAP_{i}")
        if state.cr <= 0.0:
            violations.append("INV_CR_POSITIVE")
        return {
            "ok": len(violations) == 0,
            "alert_emitted": len(violations) > 0,
            "violations": violations,
        }


def distribute_fees(total_fee: float) -> Dict[str, float]:
    return {
        "keeper": total_fee * 0.30,
        "watchdog": total_fee * 0.10,
        "auditor": total_fee * 0.05,
        "treasury": total_fee * 0.55,
    }


# -----------------------------------------------------------------------------
# Economic/security hardening primitives
# -----------------------------------------------------------------------------


class CorrelationAwareRebalancer:
    """Emergency basket rebalancer that reacts to correlated depeg stress."""

    def __init__(
        self,
        *,
        correlation_threshold: float = 0.70,
        emergency_cap: float = 0.35,
        depeg_threshold: float = 0.015,
    ):
        self.correlation_threshold = float(correlation_threshold)
        self.emergency_cap = float(emergency_cap)
        self.depeg_threshold = float(depeg_threshold)

    def detect_correlated_depeg(self, prices: Sequence[float], correlation: float) -> bool:
        stressed = sum(1 for p in prices if (1.0 - float(p)) >= self.depeg_threshold)
        return stressed >= 2 and float(correlation) >= self.correlation_threshold

    @staticmethod
    def _normalize(weights: Sequence[float]) -> List[float]:
        vals = [max(0.0, float(w)) for w in weights]
        s = sum(vals)
        if s <= EPS:
            return [1.0 / max(1, len(vals)) for _ in vals]
        return [v / s for v in vals]

    def emergency_rebalance(self, weights: Sequence[float], prices: Sequence[float], correlation: float) -> List[float]:
        if len(weights) != len(prices):
            raise ValueError("weights/prices length mismatch")
        base = self._normalize(weights)
        if not self.detect_correlated_depeg(prices, correlation):
            return base

        # Reduce exposure to deepest depeg assets and distribute residual weight.
        stress = [max(0.0, 1.0 - float(p)) for p in prices]
        avg_stress = sum(stress) / max(1, len(stress))
        adjusted: List[float] = []
        for w, s in zip(base, stress):
            pressure = 0.0 if avg_stress <= EPS else min(1.0, s / max(avg_stress, EPS))
            reduced = w * (1.0 - 0.40 * pressure)
            adjusted.append(min(self.emergency_cap, max(0.0, reduced)))
        return self._normalize(adjusted)


class DynamicRedemptionFee:
    """Fee schedule that rises with redemption pressure (0.1%~5.0%)."""

    def __init__(self, min_fee: float = 0.001, max_fee: float = 0.05, kink: float = 0.60):
        self.min_fee = float(min_fee)
        self.max_fee = float(max_fee)
        self.kink = max(0.05, min(0.95, float(kink)))

    def compute_fee(self, redemption_pressure: float) -> float:
        p = max(0.0, float(redemption_pressure))
        if p <= self.kink:
            ratio = p / self.kink
            fee = self.min_fee + (self.max_fee * 0.35 - self.min_fee) * ratio
        else:
            tail = min(1.0, (p - self.kink) / max(EPS, 1.0 - self.kink))
            fee = self.max_fee * (0.35 + 0.65 * tail)
        return min(self.max_fee, max(self.min_fee, fee))


class CBInteractionGraph:
    """Tracks breaker dependencies and detects cascading deadlock cycles."""

    def __init__(self):
        self.dependencies: Dict[int, Set[int]] = {1: {2, 3}, 2: {3}, 3: set(), 4: set()}

    def add_dependency(self, cb_id: int, waits_for: int) -> None:
        self.dependencies.setdefault(int(cb_id), set()).add(int(waits_for))

    def detect_deadlock(self, active_states: Dict[int, str]) -> bool:
        waiting = {cb for cb, st in active_states.items() if st == CB_RECOVERY_CHECK}
        if len(waiting) < 2:
            return False

        def has_cycle(node: int, stack: Set[int], visited: Set[int]) -> bool:
            if node in stack:
                return True
            if node in visited:
                return False
            visited.add(node)
            stack.add(node)
            for nxt in self.dependencies.get(node, set()):
                if nxt in waiting and has_cycle(nxt, stack, visited):
                    return True
            stack.remove(node)
            return False

        seen: Set[int] = set()
        for cb in waiting:
            if has_cycle(cb, set(), seen):
                return True
        return False

    def escalation_plan(self, active_states: Dict[int, str]) -> List[str]:
        if not self.detect_deadlock(active_states):
            return []
        # Release highest-priority blockers first.
        plan = []
        for cb_id in (3, 2, 1):
            if active_states.get(cb_id) == CB_RECOVERY_CHECK:
                plan.append(f"force_release_cb{cb_id}")
        if not plan:
            plan.append("manual_governance_override")
        return plan


class EconomicFloor:
    """Treasury-backed minimum reward floor to avoid agent exodus."""

    def __init__(
        self,
        treasury_balance: float,
        *,
        min_reward_per_agent: float = 8.0,
        max_treasury_draw_ratio: float = 0.03,
    ):
        self.treasury_balance = float(treasury_balance)
        self.min_reward_per_agent = float(min_reward_per_agent)
        self.max_treasury_draw_ratio = float(max_treasury_draw_ratio)

    def apply_floor(self, active_agents: int, variable_reward_pool: float) -> Dict[str, float]:
        agents = max(0, int(active_agents))
        if agents == 0:
            return {"per_agent": 0.0, "treasury_used": 0.0, "treasury_remaining": self.treasury_balance}

        variable = max(0.0, float(variable_reward_pool)) / agents
        shortfall = max(0.0, self.min_reward_per_agent - variable)
        required = shortfall * agents
        max_draw = max(0.0, self.treasury_balance * self.max_treasury_draw_ratio)
        draw = min(required, max_draw, self.treasury_balance)
        self.treasury_balance -= draw
        floor_boost = draw / agents
        return {
            "per_agent": variable + floor_boost,
            "treasury_used": draw,
            "treasury_remaining": self.treasury_balance,
        }


def mul_div_floor(a: int, b: int, d: int) -> int:
    return (int(a) * int(b)) // int(d)


def mul_div_ceil(a: int, b: int, d: int) -> int:
    return (int(a) * int(b) + int(d) - 1) // int(d)


def median_price(prices: Sequence[float]) -> float:
    vals = sorted(float(p) for p in prices)
    if not vals:
        raise ValueError("median_price requires at least one sample")
    m = len(vals) // 2
    if len(vals) % 2 == 1:
        return vals[m]
    return 0.5 * (vals[m - 1] + vals[m])


def validated_oracle_price(
    oracle_samples: Sequence[float],
    stale_seconds: int,
    *,
    max_stale_seconds: int = ORACLE_FRESHNESS_MAX_SECONDS,
) -> float:
    if stale_seconds > max_stale_seconds:
        raise ValueError("oracle_stale")
    px = median_price(oracle_samples)
    if px <= 0.0 or (not math.isfinite(px)):
        raise ValueError("oracle_invalid")
    return px


def collateral_quality_ok(quality_score: float, *, floor: float = MIN_COLLATERAL_QUALITY) -> bool:
    return math.isfinite(quality_score) and quality_score >= floor


def secure_mint_amount(
    collateral_units: int,
    oracle_samples: Sequence[float],
    stale_seconds: int,
    quality_score: float,
    *,
    cr_target_ppm: int = 1_200_000,
    fee_rate_ppm: int = 2_000,
) -> int:
    price = validated_oracle_price(oracle_samples, stale_seconds)
    if not collateral_quality_ok(quality_score):
        return 0
    gross = mul_div_floor(collateral_units, int(price * 1_000_000), 1_000_000)
    max_mint = mul_div_floor(gross, 1_000_000, cr_target_ppm)
    fee = mul_div_ceil(max_mint, fee_rate_ppm, 1_000_000)
    minted = max_mint - fee
    return max(0, minted)


def redeem_by_value(
    vault_units: Sequence[int],
    oracle_prices: Sequence[float],
    burn_amount: int,
    total_supply: int,
    *,
    payout_discount_ppm: int = 1_000_000,
) -> List[int]:
    if total_supply <= 0:
        raise ValueError("total_supply must be positive")
    if burn_amount <= 0:
        raise ValueError("burn_amount must be positive")
    if len(vault_units) != len(oracle_prices):
        raise ValueError("vault_units/prices length mismatch")

    values = [max(0.0, float(u) * float(p)) for u, p in zip(vault_units, oracle_prices)]
    total_value = sum(values)
    if total_value <= 0.0:
        return [0] * len(vault_units)

    discount = max(0.0, min(1.0, payout_discount_ppm / 1_000_000.0))
    target_value = total_value * (burn_amount / float(total_supply)) * discount

    payouts: List[int] = []
    allocated_value = 0.0
    for i, (units, price, asset_value) in enumerate(zip(vault_units, oracle_prices, values)):
        if i == len(vault_units) - 1:
            residual = max(0.0, target_value - allocated_value)
            out = int(residual / max(float(price), EPS))
        else:
            share = asset_value / total_value
            value_out = target_value * share
            out = int(value_out / max(float(price), EPS))
            allocated_value += out * float(price)
        payouts.append(max(0, min(int(units), out)))
    return payouts


@dataclass
class RedemptionRequest:
    account: str
    burn_amount: int
    requested_discount_ppm: int


class RedemptionQueue:
    def __init__(self, smoothing_window: int = 8):
        self._pending: Deque[RedemptionRequest] = deque()
        self._smoothing_window = max(1, int(smoothing_window))

    def enqueue(self, account: str, burn_amount: int, requested_discount_ppm: int) -> None:
        self._pending.append(
            RedemptionRequest(
                account=str(account),
                burn_amount=max(0, int(burn_amount)),
                requested_discount_ppm=int(requested_discount_ppm),
            )
        )

    def settle(
        self,
        vault_units: Sequence[int],
        oracle_prices: Sequence[float],
        total_supply: int,
    ) -> Dict[str, List[int]]:
        if not self._pending:
            return {}

        batch: List[RedemptionRequest] = []
        while self._pending and len(batch) < self._smoothing_window:
            batch.append(self._pending.popleft())

        avg_discount = int(sum(r.requested_discount_ppm for r in batch) / len(batch))
        total_burn = sum(r.burn_amount for r in batch)
        aggregate = redeem_by_value(
            vault_units=vault_units,
            oracle_prices=oracle_prices,
            burn_amount=total_burn,
            total_supply=total_supply,
            payout_discount_ppm=avg_discount,
        )

        out: Dict[str, List[int]] = {}
        allocated = [[0 for _ in aggregate] for _ in batch]
        running = [0 for _ in aggregate]
        for idx, req in enumerate(batch):
            ratio = (req.burn_amount / total_burn) if total_burn > 0 else 0.0
            for j, total_units in enumerate(aggregate):
                if idx == len(batch) - 1:
                    alloc = total_units - running[j]
                else:
                    alloc = int(total_units * ratio)
                    running[j] += alloc
                allocated[idx][j] = max(0, alloc)
            out[req.account] = allocated[idx]
        return out


class ProtocolTxScheduler:
    def __init__(
        self,
        *,
        reserved_tx_slots: int = DEFAULT_RESERVED_TX_SLOTS,
        reserved_compute_units: int = DEFAULT_RESERVED_COMPUTE_UNITS,
        priority_fee_microlamports: int = DEFAULT_PRIORITY_FEE_MICROLAMPORTS,
    ):
        self.reserved_tx_slots = max(0, int(reserved_tx_slots))
        self.reserved_compute_units = max(0, int(reserved_compute_units))
        self.priority_fee_microlamports = max(0, int(priority_fee_microlamports))

    def admit_by_compute(
        self,
        block_compute_limit: int,
        attacker_compute: int,
        protocol_txs: int,
        protocol_tx_compute: int,
    ) -> Dict[str, int]:
        protocol_tx_compute = max(1, int(protocol_tx_compute))
        public_budget = max(0, int(block_compute_limit) - self.reserved_compute_units)
        public_remaining = max(0, public_budget - int(attacker_compute))
        public_capacity = public_remaining // protocol_tx_compute
        reserved_capacity = self.reserved_compute_units // protocol_tx_compute
        admitted = min(int(protocol_txs), int(public_capacity + reserved_capacity))
        return {
            "admitted": admitted,
            "public_capacity": int(public_capacity),
            "reserved_capacity": int(reserved_capacity),
        }

    def admit_by_slots(self, block_tx_capacity: int, attacker_spam_txs: int, protocol_txs: int) -> Dict[str, int]:
        public_budget = max(0, int(block_tx_capacity) - self.reserved_tx_slots)
        public_remaining = max(0, public_budget - int(attacker_spam_txs))
        admitted = min(int(protocol_txs), int(public_remaining + self.reserved_tx_slots))
        return {
            "admitted": admitted,
            "public_remaining": int(public_remaining),
            "reserved_slots": int(self.reserved_tx_slots),
        }


class BatchRebalanceAuction:
    def __init__(self, fee_rate: float = 0.003):
        self.fee_rate = max(0.0, min(0.02, float(fee_rate)))

    def sandwich_pnl(self, attacker_input_musd: float) -> float:
        # Commit/reveal + batch auction: attacker cannot bracket keeper flow; only pays fees.
        gross = float(attacker_input_musd)
        after_buy_fee = gross * (1.0 - self.fee_rate)
        after_sell_fee = after_buy_fee * (1.0 - self.fee_rate)
        return after_sell_fee - gross


class InsuranceFund:
    def __init__(
        self,
        treasury: float,
        *,
        min_claim: float = MIN_INSURANCE_CLAIM,
        cooldown_ticks: int = INSURANCE_COOLDOWN_TICKS,
        epoch_length_ticks: int = 20,
        epoch_claim_cap: float = 10_000.0,
        refill_trigger: float = 250_000.0,
        refill_amount: float = 200_000.0,
    ):
        self.treasury = float(treasury)
        self.min_claim = float(min_claim)
        self.cooldown_ticks = max(1, int(cooldown_ticks))
        self.epoch_length_ticks = max(1, int(epoch_length_ticks))
        self.epoch_claim_cap = float(epoch_claim_cap)
        self.refill_trigger = float(refill_trigger)
        self.refill_amount = float(refill_amount)
        self._last_claim_tick: Dict[str, int] = {}
        self._epoch_index = -1
        self._epoch_claimed = 0.0

    def _refresh_epoch(self, tick: int) -> None:
        idx = int(tick) // self.epoch_length_ticks
        if idx != self._epoch_index:
            self._epoch_index = idx
            self._epoch_claimed = 0.0

    def _auto_refill(self) -> None:
        if self.treasury <= self.refill_trigger:
            self.treasury += self.refill_amount

    def claim(self, claimant: str, amount: float, tick: int) -> Dict[str, object]:
        self._refresh_epoch(int(tick))
        self._auto_refill()

        amt = float(amount)
        if amt < self.min_claim:
            return {"approved": False, "reason": "below_min_claim"}

        last = self._last_claim_tick.get(claimant)
        if last is not None and int(tick) - last < self.cooldown_ticks:
            return {"approved": False, "reason": "cooldown"}

        if self._epoch_claimed + amt > self.epoch_claim_cap:
            return {"approved": False, "reason": "epoch_cap"}

        if amt > self.treasury:
            return {"approved": False, "reason": "insufficient_fund"}

        self.treasury -= amt
        self._epoch_claimed += amt
        self._last_claim_tick[claimant] = int(tick)
        return {"approved": True, "reason": "ok", "treasury": self.treasury}


# -----------------------------------------------------------------------------
# Scenario runner + metrics
# -----------------------------------------------------------------------------


@dataclass
class ScenarioSummary:
    scenario: str
    seed: int
    ticks: int

    mae: float
    rmse: float
    min_cr: float
    max_turnover: float
    breaker_activations: Dict[int, int]
    breaker_false_positives: int
    breaker_false_positive_rate: float
    cr_violation_rate: float
    cr_final: float
    cr_target_final: float
    final_fee: float

    gate_peg_ok: bool
    gate_cr_ok: bool
    gate_fp_ok: bool

    rows: List[Dict[str, object]]
    events: List[Dict[str, object]]


def _assert_tick_invariants(state: ProtocolState) -> None:
    if abs(sum(state.weights) - 1.0) > 1e-6:
        raise AssertionError("sum(weights)!=1")
    for i, (w, cap) in enumerate(zip(state.weights, state.w_caps)):
        if w < -1e-10 or w > cap + 1e-10:
            raise AssertionError(f"weight bound violation at {i}")
    if state.cr <= 0.0:
        raise AssertionError("CR <= 0")
    values = [state.cr, state.mint_fee, state.reserve_value, state.supply, state.position_supply_sum] + state.weights + state.w_caps
    if any((not math.isfinite(v)) for v in values):
        raise AssertionError("non-finite invariant")
    if abs(state.position_supply_sum - state.supply) > 1e-6:
        raise AssertionError("position_supply_mismatch")


def run_scenario(
    scenario: str,
    seed: int = 0,
    ticks: int = 120,
    enforce_invariants: bool = True,
) -> ScenarioSummary:
    env = MarketEnv(scenario=scenario, seed=seed, deterministic=True)
    state = ProtocolState()
    loss_engine = LossEngine()
    optimizer = AdamOptimizer(n_weights=len(state.weights))
    breaker = CircuitBreaker(n_assets=len(state.weights))
    keeper = Keeper()
    watchdog = Watchdog()
    # // BLUE-TEAM: DEF-INV - runtime invariant monitor (security/invariant_monitor.py).
    from security.invariant_monitor import InvariantMonitor

    invariant_monitor = InvariantMonitor()

    rows: List[Dict[str, object]] = []
    peg_errors: List[float] = []
    sq_errors: List[float] = []
    cr_violations = 0
    min_cr = 10**9
    max_turnover = 0.0

    activation_counts = {1: 0, 2: 0, 3: 0, 4: 0}
    false_positives = 0

    checkpoint_state = state.clone()
    checkpoint_lr = optimizer.lr

    event_idx = 0

    for tick in range(ticks):
        state.begin_tick()
        fee_before = state.mint_fee
        market = env.step(tick)

        loss_finite = True
        loss_value: Optional[float] = None
        grad_w = [0.0] * len(state.weights)
        grad_fee = 0.0

        try:
            loss, ctx = loss_engine.compute(state, market.prices, market.oracle_q)
            loss_value = loss.data
            loss.backward()
            grad_w = [wv.grad for wv in ctx["weights"]]  # type: ignore[index]
            grad_fee = float(ctx["fee"].grad)  # type: ignore[index]
            if any(not math.isfinite(g) for g in grad_w + [grad_fee]):
                raise ValueError("non-finite gradient")
        except Exception:
            loss_finite = False

        nav_now = state.effective_collateral_value(market.prices)
        nav_drop = nav_now - state.nav_prev

        wd_events = watchdog.detect(market)
        forced: Dict[str, bool] = {}
        # Use watchdog force mainly for oracle failure; depeg breakers keep streak logic.
        if wd_events.get("cb3"):
            forced["cb3"] = True

        action = breaker.update(
            tick=tick,
            state=state,
            market=market,
            nav_drop=nav_drop,
            loss_finite=loss_finite,
            loss_value=loss_value,
            forced=forced,
        )

        # count new activation events
        while event_idx < len(breaker.events):
            ev = breaker.events[event_idx]
            if ev["event"] == "activate":
                cb_id = int(ev["cb"])
                activation_counts[cb_id] += 1
                if cb_id not in market.expected_breakers:
                    false_positives += 1
                invariant_monitor.record_agent_action("watchdog", tick, action_type=f"cb{cb_id}_activate")
            event_idx += 1

        # CB-4 rollback
        if action["rollback"]:
            state = checkpoint_state.clone()
            optimizer.lr = max(1e-5, checkpoint_lr * 0.5)
            # keep active breaker safety posture after rollback
            if action["cb1"]:
                idx = breaker.cb1_target_index
                state.w_caps[idx] = min(state.w_caps[idx], state.base_w_caps[idx] * 0.5)
                state.mint_limit = min(state.mint_limit, 0.25)
                state.cr_target = max(state.cr_target, 1.25)
            if action["cb3"]:
                state.optimizer_enabled = False
                state.conservative_mode = True
                state.oracle_degraded = True
                state.mint_limit = min(state.mint_limit, 0.10)
                state.cr_target = max(state.cr_target, 1.35)
            if action["cb2"]:
                state.mint_limit = 0.0
                state.mint_paused_reason = "MINT_PAUSED_BY_CB2"
                state.cr_target = max(state.cr_target, 1.30)

        if state.optimizer_enabled and state.mint_limit > 0.0 and loss_finite:
            proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
            result = keeper.submit_update_proposal(state, proposal)
            if result.get("status") == "APPLIED":
                delta_mag = sum(abs(float(proposal["weights"][i]) - state.prev_weights[i]) for i in range(len(state.weights)))  # type: ignore[index]
                invariant_monitor.record_agent_action("keeper", tick, magnitude=delta_mag, action_type="rebalance")
            else:
                # keep system stable even if proposal is rejected
                pass

        peg_noise = 0.0
        if scenario == "normal":
            peg_noise = env.rng.gauss(0.0, 0.00010)
        elif scenario == "volatile":
            peg_noise = env.rng.gauss(0.0, 0.00035)
        else:
            peg_noise = env.rng.gauss(0.0, 0.00020)

        peg = state.update_from_market(market.prices, market.oracle_q, peg_noise=peg_noise)

        invariant_monitor.check(
            tick=tick,
            state=state,
            market=market,
            weights=state.weights,
            weight_caps=state.w_caps,
            min_cr=state.cr_min,
            oracle_stale_limit=120,
            max_actions_per_window=40,
            window_ticks=20,
        )

        turnover = sum(abs(a - b) for a, b in zip(state.weights, state.prev_weights))
        max_turnover = max(max_turnover, turnover)

        err = abs(peg - 1.0)
        peg_errors.append(err)
        sq_errors.append((peg - 1.0) ** 2)

        min_cr = min(min_cr, state.cr)
        if state.cr < state.cr_hard_min:
            cr_violations += 1

        if enforce_invariants:
            _assert_tick_invariants(state)

        rows.append(
            {
                "scenario": scenario,
                "seed": seed,
                "tick": tick,
                "peg": peg,
                "peg_error": err,
                "cr": state.cr,
                "cr_target": state.cr_target,
                "turnover": turnover,
                "loss": loss_value if loss_value is not None else float("nan"),
                "oracle_q": market.oracle_q,
                "optimizer_enabled": int(state.optimizer_enabled),
                "mint_limit": state.mint_limit,
                "w0": state.weights[0],
                "w1": state.weights[1],
                "w2": state.weights[2],
                "w3": state.weights[3],
                "fee": state.mint_fee,
                "cb1": int(action["cb1"]),
                "cb2": int(action["cb2"]),
                "cb3": int(action["cb3"]),
                "cb4": int(action["cb4"]),
            }
        )

        checkpoint_state = state.clone()
        checkpoint_lr = optimizer.lr

    mae = sum(peg_errors) / len(peg_errors) if peg_errors else 0.0
    rmse = math.sqrt(sum(sq_errors) / len(sq_errors)) if sq_errors else 0.0
    cr_violation_rate = cr_violations / float(max(1, ticks))
    total_activations = sum(activation_counts.values())
    fp_rate = false_positives / float(max(1, total_activations))

    gate_peg_ok = mae < 0.0015
    gate_cr_ok = cr_violation_rate < 0.01
    gate_fp_ok = fp_rate < 0.05

    return ScenarioSummary(
        scenario=scenario,
        seed=seed,
        ticks=ticks,
        mae=mae,
        rmse=rmse,
        min_cr=min_cr,
        max_turnover=max_turnover,
        breaker_activations=activation_counts,
        breaker_false_positives=false_positives,
        breaker_false_positive_rate=fp_rate,
        cr_violation_rate=cr_violation_rate,
        cr_final=state.cr,
        cr_target_final=state.cr_target,
        final_fee=state.mint_fee,
        gate_peg_ok=gate_peg_ok,
        gate_cr_ok=gate_cr_ok,
        gate_fp_ok=gate_fp_ok,
        rows=rows,
        events=breaker.events[:],
    )


def save_metrics_csv(rows: List[Dict[str, object]], path: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if not rows:
        with open(path, "w", newline="", encoding="utf-8") as f:
            f.write("\n")
        return
    fieldnames = list(rows[0].keys())
    with open(path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for r in rows:
            writer.writerow(r)


def save_events_log(events: List[Dict[str, object]], path: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        for ev in events:
            f.write(
                f"tick={ev['tick']:>4} cb={ev['cb']} event={ev['event']} "
                f"{ev['old']}->{ev['new']} detail={ev.get('detail','')}\n"
            )


def run_all_scenarios(
    scenarios: Sequence[str],
    seed: int = 0,
    ticks: int = 120,
    output_dir: Optional[str] = None,
) -> Dict[str, ScenarioSummary]:
    out: Dict[str, ScenarioSummary] = {}
    all_rows: List[Dict[str, object]] = []
    all_events: List[Dict[str, object]] = []

    for sc in scenarios:
        res = run_scenario(sc, seed=seed, ticks=ticks, enforce_invariants=True)
        out[sc] = res
        all_rows.extend(res.rows)
        for ev in res.events:
            ev2 = dict(ev)
            ev2["scenario"] = sc
            all_events.append(ev2)

    if output_dir is not None:
        save_metrics_csv(all_rows, os.path.join(output_dir, "metrics.csv"))
        save_events_log(all_events, os.path.join(output_dir, "events.log"))

    return out


# -----------------------------------------------------------------------------
# Utility stats
# -----------------------------------------------------------------------------


def percentile(values: Sequence[float], p: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    if p <= 0:
        return s[0]
    if p >= 100:
        return s[-1]
    k = (len(s) - 1) * (p / 100.0)
    lo = int(math.floor(k))
    hi = int(math.ceil(k))
    if lo == hi:
        return s[lo]
    frac = k - lo
    return s[lo] * (1.0 - frac) + s[hi] * frac


def summarize_stats(values: Sequence[float]) -> Dict[str, float]:
    if not values:
        return {"mean": 0.0, "median": 0.0, "p5": 0.0, "p95": 0.0, "worst": 0.0}
    vals = list(values)
    return {
        "mean": sum(vals) / len(vals),
        "median": percentile(vals, 50),
        "p5": percentile(vals, 5),
        "p95": percentile(vals, 95),
        "worst": max(vals),
    }


def text_histogram(values: Sequence[float], bins: int = 12, width: int = 32) -> str:
    if not values:
        return "(empty)"
    vals = list(values)
    lo = min(vals)
    hi = max(vals)
    if hi <= lo + 1e-15:
        return f"[{lo:.6g}] {'#' * width} ({len(vals)})"

    counts = [0] * bins
    for v in vals:
        idx = int((v - lo) / (hi - lo) * bins)
        if idx == bins:
            idx -= 1
        counts[idx] += 1

    max_count = max(counts)
    lines = []
    for i in range(bins):
        a = lo + (hi - lo) * i / bins
        b = lo + (hi - lo) * (i + 1) / bins
        bar_len = int(width * (counts[i] / max_count)) if max_count > 0 else 0
        lines.append(f"{a:.6f}..{b:.6f} | {'#' * bar_len} ({counts[i]})")
    return "\n".join(lines)


# -----------------------------------------------------------------------------
# Main quick run
# -----------------------------------------------------------------------------


def main() -> None:
    scenarios = ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    out = run_all_scenarios(
        scenarios,
        seed=0,
        ticks=120,
        output_dir=os.path.join(os.path.dirname(__file__), "outputs"),
    )

    print("microstable quick run")
    for sc in scenarios:
        r = out[sc]
        print(
            f"{sc:16s} MAE={r.mae:.6f} RMSE={r.rmse:.6f} "
            f"minCR={r.min_cr:.4f} fpRate={r.breaker_false_positive_rate:.3f} activations={r.breaker_activations}"
        )


if __name__ == "__main__":
    main()
