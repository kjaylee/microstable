"""
microstable.py
Single-file, dependency-free simulation kernel for the microstable protocol.
Python 3.10+
"""

# === Section 1: Value (autograd) ===

import math
import random
from collections import deque
from dataclasses import dataclass, field
from statistics import mean, median, pstdev
from typing import Callable, Dict, List, Optional, Sequence, Tuple


EPS = 1e-7
DELTA_W_MAX = 0.02
DELTA_FEE_MAX = 0.001
GRAD_CLIP_NORM = 1.0
CR_HARD_MIN = 1.05

ASSETS = ["USDC", "USDT", "DAI", "USDS"]
INITIAL_WEIGHTS = [0.40, 0.30, 0.20, 0.10]
BASE_W_CAPS = [0.55, 0.45, 0.45, 0.35]


class Value:
    """Scalar autograd value with micrograd-style reverse mode."""

    def __init__(self, data: float, _children: Tuple["Value", ...] = (), _op: str = "", label: Optional[str] = None):
        data = float(data)
        if not math.isfinite(data):
            raise ValueError(f"non-finite Value data: {data}")
        self.data = data
        self.grad = 0.0
        self._prev = set(_children)
        self._op = _op
        self._backward: Callable[[], None] = lambda: None
        self.label = label

    def __repr__(self) -> str:
        return f"Value(data={self.data:.8f}, grad={self.grad:.8f}, op='{self._op}')"

    @staticmethod
    def _coerce(other: float | "Value") -> "Value":
        return other if isinstance(other, Value) else Value(float(other))

    @staticmethod
    def _check_finite(x: float, where: str) -> None:
        if not math.isfinite(x):
            raise ValueError(f"non-finite detected in {where}: {x}")

    def __add__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        out = Value(self.data + other.data, (self, other), "+")

        def _backward() -> None:
            self.grad += 1.0 * out.grad
            other.grad += 1.0 * out.grad
            self._check_finite(self.grad, "add-self-grad")
            self._check_finite(other.grad, "add-other-grad")

        out._backward = _backward
        return out

    def __radd__(self, other: float | "Value") -> "Value":
        return self + other

    def __neg__(self) -> "Value":
        out = Value(-self.data, (self,), "neg")

        def _backward() -> None:
            self.grad += -1.0 * out.grad
            self._check_finite(self.grad, "neg-grad")

        out._backward = _backward
        return out

    def __sub__(self, other: float | "Value") -> "Value":
        return self + (-self._coerce(other))

    def __rsub__(self, other: float | "Value") -> "Value":
        return self._coerce(other) - self

    def __mul__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        out = Value(self.data * other.data, (self, other), "*")

        def _backward() -> None:
            self.grad += other.data * out.grad
            other.grad += self.data * out.grad
            self._check_finite(self.grad, "mul-self-grad")
            self._check_finite(other.grad, "mul-other-grad")

        out._backward = _backward
        return out

    def __rmul__(self, other: float | "Value") -> "Value":
        return self * other

    def __truediv__(self, other: float | "Value") -> "Value":
        other = self._coerce(other)
        denom = other.data
        if abs(denom) < EPS:
            denom = denom + EPS
        out = Value(self.data / denom, (self, other), "/")

        def _backward() -> None:
            self.grad += (1.0 / denom) * out.grad
            other.grad += (-self.data / (denom * denom)) * out.grad
            self._check_finite(self.grad, "div-self-grad")
            self._check_finite(other.grad, "div-other-grad")

        out._backward = _backward
        return out

    def __rtruediv__(self, other: float | "Value") -> "Value":
        return self._coerce(other) / self

    def __pow__(self, power: float) -> "Value":
        power = float(power)
        is_integer_power = abs(power - round(power)) < 1e-12
        if self.data <= 0.0 and not is_integer_power:
            raise ValueError("pow base must be > 0 for non-integer powers")
        out = Value(self.data ** power, (self,), f"**{power}")

        def _backward() -> None:
            # x<=0 defense: allow integer powers safely, block undefined fractional derivatives.
            if self.data == 0.0 and power < 1.0:
                local = 0.0
            else:
                local = power * (self.data ** (power - 1.0))
            self.grad += local * out.grad
            self._check_finite(self.grad, "pow-grad")

        out._backward = _backward
        return out

    def tanh(self) -> "Value":
        t = math.tanh(self.data)
        out = Value(t, (self,), "tanh")

        def _backward() -> None:
            self.grad += (1.0 - t * t) * out.grad
            self._check_finite(self.grad, "tanh-grad")

        out._backward = _backward
        return out

    def exp(self) -> "Value":
        x = max(min(self.data, 20.0), -20.0)
        e = math.exp(x)
        out = Value(e, (self,), "exp")

        def _backward() -> None:
            self.grad += e * out.grad
            self._check_finite(self.grad, "exp-grad")

        out._backward = _backward
        return out

    def log(self) -> "Value":
        x = self.data if self.data > EPS else EPS
        out = Value(math.log(x), (self,), "log")

        def _backward() -> None:
            local = (1.0 / x) if self.data > EPS else 0.0
            self.grad += local * out.grad
            self._check_finite(self.grad, "log-grad")

        out._backward = _backward
        return out

    def relu(self) -> "Value":
        val = self.data if self.data > 0.0 else 0.0
        out = Value(val, (self,), "relu")

        def _backward() -> None:
            local = 1.0 if self.data > 0.0 else 0.0  # relu(0) -> 0
            self.grad += local * out.grad
            self._check_finite(self.grad, "relu-grad")

        out._backward = _backward
        return out

    def clamp(self, lo: float, hi: float) -> "Value":
        if lo > hi:
            raise ValueError("invalid clamp range")
        val = min(max(self.data, lo), hi)
        out = Value(val, (self,), "clamp")

        def _backward() -> None:
            local = 1.0 if (lo < self.data < hi) else 0.0
            self.grad += local * out.grad
            self._check_finite(self.grad, "clamp-grad")

        out._backward = _backward
        return out

    def backward(self) -> None:
        topo: List[Value] = []
        visited = set()

        def build(v: Value) -> None:
            if v not in visited:
                visited.add(v)
                for child in v._prev:
                    build(child)
                topo.append(v)

        build(self)
        for node in topo:
            node.grad = 0.0
        self.grad = 1.0

        for node in reversed(topo):
            node._backward()
            self._check_finite(node.grad, f"backward-{node._op}")


def v_tanh(x: Value) -> Value:
    return x.tanh()


def v_exp(x: Value) -> Value:
    return x.exp()


def v_log(x: Value) -> Value:
    return x.log()


def v_relu(x: Value) -> Value:
    return x.relu()


# === Section 2: MarketEnv (price/oracle simulator) ===


@dataclass
class MarketTick:
    prices: List[float]
    oracle_q: float
    stale_seconds: int
    divergence: float
    expected_stress: bool


class MarketEnv:
    """Scenario-driven synthetic market generator."""

    def __init__(self, scenario: str, seed: int = 0):
        self.scenario = scenario
        self.rng = random.Random(seed)
        self.prices = [1.0, 1.0, 1.0, 1.0]

    def _base_vol(self) -> float:
        return {
            "normal": 0.00035,
            "single_depeg": 0.0006,
            "multi_depeg": 0.0008,
            "volatile": 0.0025,
            "gradient_attack": 0.0012,
            "oracle_failure": 0.0008,
        }.get(self.scenario, 0.0007)

    def _shock(self, tick: int, i: int) -> float:
        if self.scenario == "single_depeg":
            if i == 1 and 20 <= tick <= 24:
                return -0.025
        if self.scenario == "multi_depeg":
            if i in (0, 1) and 20 <= tick <= 26:
                return -0.08
        if self.scenario == "gradient_attack":
            if tick in (18, 19, 20, 21):
                return 0.045 if (i % 2 == 0) else -0.045
        if self.scenario == "volatile":
            if tick % 13 == 0:
                return self.rng.uniform(-0.015, 0.015)
        return 0.0

    def step(self, tick: int) -> MarketTick:
        vol = self._base_vol()
        expected_stress = False

        for i in range(len(self.prices)):
            noise = self.rng.gauss(0.0, vol)
            revert = 0.2 * (1.0 - self.prices[i])
            shock = self._shock(tick, i)
            if shock != 0.0:
                expected_stress = True
            p = self.prices[i] + noise + revert + shock
            self.prices[i] = min(max(p, 0.5), 1.5)

        stale = 0
        div = abs(self.rng.gauss(0.0, 0.001))
        if self.scenario == "oracle_failure" and 20 <= tick <= 42:
            stale = 180
            div = 0.04
            expected_stress = True

        depeg_count = sum(1 for p in self.prices if abs(p - 1.0) > 0.02)
        expected_stress = expected_stress or depeg_count >= 1

        q = 1.0 - min(0.8, stale / 500.0) - min(0.7, div / 0.08)
        q += self.rng.gauss(0.0, 0.003)
        q = min(1.0, max(0.0, q))

        return MarketTick(prices=self.prices[:], oracle_q=q, stale_seconds=stale, divergence=div, expected_stress=expected_stress)


# === Section 3: ProtocolState (collateral/supply/params) ===


@dataclass
class ProtocolState:
    assets: List[str] = field(default_factory=lambda: ASSETS[:])
    weights: List[float] = field(default_factory=lambda: INITIAL_WEIGHTS[:])
    prev_weights: List[float] = field(default_factory=lambda: INITIAL_WEIGHTS[:])
    base_w_caps: List[float] = field(default_factory=lambda: BASE_W_CAPS[:])
    w_caps: List[float] = field(default_factory=lambda: BASE_W_CAPS[:])
    risk_haircuts: List[float] = field(default_factory=lambda: [0.002, 0.003, 0.002, 0.004])

    supply: float = 1_000_000.0
    reserve_value: float = 1_280_000.0
    cr_target: float = 1.20
    cr_hard_min: float = CR_HARD_MIN
    cr: float = 1.28

    mint_fee: float = 0.002
    redeem_fee: float = 0.002

    mint_limit: float = 1.0
    mint_paused_reason: str = ""
    optimizer_enabled: bool = True
    conservative_mode: bool = False
    oracle_degraded: bool = False

    nav_prev: float = 1.0
    nav_window: List[float] = field(default_factory=lambda: [1.0, 1.0, 1.0, 1.0])
    peg_history: List[float] = field(default_factory=list)
    cr_history: List[float] = field(default_factory=list)

    def clone(self) -> "ProtocolState":
        return ProtocolState(
            assets=self.assets[:],
            weights=self.weights[:],
            prev_weights=self.prev_weights[:],
            base_w_caps=self.base_w_caps[:],
            w_caps=self.w_caps[:],
            risk_haircuts=self.risk_haircuts[:],
            supply=self.supply,
            reserve_value=self.reserve_value,
            cr_target=self.cr_target,
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
            nav_window=self.nav_window[:],
            peg_history=self.peg_history[:],
            cr_history=self.cr_history[:],
        )

    def begin_tick(self) -> None:
        self.prev_weights = self.weights[:]

    def reset_dynamic_policy(self) -> None:
        self.w_caps = self.base_w_caps[:]
        self.mint_limit = 1.0
        self.mint_paused_reason = ""
        self.optimizer_enabled = True
        self.conservative_mode = False
        self.oracle_degraded = False

    def effective_nav(self, prices: Sequence[float]) -> float:
        total = 0.0
        for w, p, h in zip(self.weights, prices, self.risk_haircuts):
            total += w * p * (1.0 - h)
        return total

    def update_market_state(self, prices: Sequence[float], oracle_q: float, peg_noise: float = 0.0) -> float:
        nav = self.effective_nav(prices)
        nav_delta = nav - self.nav_prev
        self.nav_prev = nav

        self.nav_window.append(nav_delta)
        if len(self.nav_window) > 30:
            self.nav_window.pop(0)

        peg = 1.0 + 0.055 * (nav - 1.0) + 0.0005 * (oracle_q - 1.0) + peg_noise
        peg = min(max(peg, 0.90), 1.10)
        self.peg_history.append(peg)

        desired = self.cr_target + (0.03 if self.conservative_mode else 0.0)
        self.cr += 0.10 * (desired - self.cr) + 0.35 * nav_delta
        floor = max(self.cr_hard_min + 0.002, self.cr_target + 0.001)
        self.cr = min(max(self.cr, floor), 2.2)
        self.reserve_value = self.cr * self.supply
        self.cr_history.append(self.cr)
        return peg

    def apply_weights_and_fee(self, weights: Sequence[float], mint_fee: Optional[float] = None) -> None:
        self.weights = [float(x) for x in weights]
        if mint_fee is not None:
            self.mint_fee = float(mint_fee)


# === Section 4: LossEngine (objective function) ===


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
        self.turn_eps = 1e-8

    @staticmethod
    def _smooth_abs(x: Value, eps: float = 1e-8) -> Value:
        return (x * x + eps) ** 0.5

    def compute(self, state: ProtocolState, prices: Sequence[float], oracle_q: float) -> Tuple[Value, Dict[str, Value | List[Value]]]:
        wvals = [Value(w, label=f"w{i}") for i, w in enumerate(state.weights)]
        fee = Value(state.mint_fee, label="mint_fee")

        nav = Value(0.0)
        for wv, p, h in zip(wvals, prices, state.risk_haircuts):
            nav = nav + wv * (p * (1.0 - h))

        peg = Value(1.0) + (nav - 1.0) * 0.055 + Value(oracle_q - 1.0) * 0.0005
        cr_est = Value(state.cr) + (nav - 1.0) * 0.15

        peg_loss = self.c.lambda_p * ((peg - 1.0) ** 2)

        cr_gap = Value(state.cr_target) - cr_est
        hinge = cr_gap.relu()  # gradient at boundary = 0
        cr_penalty = self.c.lambda_cr * (hinge ** 2)

        recent = state.nav_window[-10:] if len(state.nav_window) >= 2 else [0.0, 0.0]
        deltas = [Value(x) for x in recent]
        deltas.append(nav - Value(state.nav_prev))
        mu = Value(0.0)
        for d in deltas:
            mu = mu + d
        mu = mu / float(len(deltas))

        var = Value(0.0)
        for d in deltas:
            diff = d - mu
            var = var + diff * diff
        var = var / float(len(deltas))
        var_term = self.c.lambda_var * var

        turnover = Value(0.0)
        for wv, wp in zip(wvals, state.prev_weights):
            turnover = turnover + self._smooth_abs(wv - wp, self.turn_eps)
        turn_term = self.c.lambda_turn * turnover

        conc = Value(0.0)
        for wv in wvals:
            conc = conc + wv * wv
        conc_term = self.c.lambda_conc * conc

        oracle_loss = Value(self.c.lambda_orc * ((1.0 - oracle_q) ** 2))
        fee_term = Value(2.0) * ((fee - 0.002) ** 2)

        total = peg_loss + cr_penalty + var_term + turn_term + conc_term + oracle_loss + fee_term

        if not math.isfinite(total.data):
            raise ValueError("non-finite loss")

        return total, {
            "weights": wvals,
            "fee": fee,
            "peg": peg,
            "cr_est": cr_est,
            "nav": nav,
            "peg_loss": peg_loss,
            "cr_penalty": cr_penalty,
            "var": var_term,
            "turnover": turn_term,
            "conc": conc_term,
            "oracle": oracle_loss,
        }


# === Section 5: Optimizer (Adam + bounded projection) ===


class AdamBoundedOptimizer:
    def __init__(self, n_weights: int, lr: float = 0.005, beta1: float = 0.9, beta2: float = 0.999, eps: float = 1e-8):
        self.nw = n_weights
        self.base_lr = lr
        self.lr = lr
        self.beta1 = beta1
        self.beta2 = beta2
        self.eps = eps
        self.t = 0
        self.m = [0.0] * (n_weights + 1)  # + fee
        self.v = [0.0] * (n_weights + 1)

    @staticmethod
    def clip_gradients(grads: Sequence[float], max_norm: float = GRAD_CLIP_NORM) -> List[float]:
        norm = math.sqrt(sum(g * g for g in grads))
        if norm <= max_norm or norm == 0.0:
            return [float(g) for g in grads]
        scale = max_norm / norm
        return [g * scale for g in grads]

    @staticmethod
    def simplex_projection(v: Sequence[float], z: float = 1.0) -> List[float]:
        if z <= 0:
            return [0.0 for _ in v]
        n = len(v)
        u = sorted(v, reverse=True)
        cssv = [0.0] * n
        run = 0.0
        rho = -1
        for i, ui in enumerate(u):
            run += ui
            cssv[i] = run
            t = (run - z) / float(i + 1)
            if ui - t > 0:
                rho = i
        if rho == -1:
            return [z / n for _ in v]
        theta = (cssv[rho] - z) / float(rho + 1)
        w = [max(vi - theta, 0.0) for vi in v]
        return w

    @classmethod
    def project_capped_simplex(cls, y: Sequence[float], caps: Sequence[float], target: float = 1.0) -> List[float]:
        n = len(y)
        if target <= 0:
            return [0.0] * n
        total_cap = sum(caps)
        if target >= total_cap - 1e-12:
            return [float(c) for c in caps]

        x = [0.0] * n
        free = list(range(n))
        remaining = float(target)
        loops = 0

        while free and loops < 10 * n:
            loops += 1
            y_free = [y[i] for i in free]
            proj = cls.simplex_projection(y_free, z=remaining)

            violated = []
            for idx_local, idx_global in enumerate(free):
                if proj[idx_local] > caps[idx_global] + 1e-12:
                    violated.append(idx_global)

            if not violated:
                for idx_local, idx_global in enumerate(free):
                    x[idx_global] = max(0.0, min(caps[idx_global], proj[idx_local]))
                break

            for idx in violated:
                x[idx] = caps[idx]
                remaining -= caps[idx]
            free = [i for i in free if i not in violated]
            if remaining <= 1e-12:
                break

        if free and remaining > 1e-12:
            room = sum(caps[i] - x[i] for i in free)
            if room > 0:
                for i in free:
                    add = remaining * ((caps[i] - x[i]) / room)
                    x[i] += add

        s = sum(x)
        if s > 0:
            x = [max(0.0, min(c, xi * target / s)) for xi, c in zip(x, caps)]

        # tiny residual fix
        resid = target - sum(x)
        if abs(resid) > 1e-10:
            for i in range(n):
                room = caps[i] - x[i]
                if resid > 0 and room > 0:
                    d = min(room, resid)
                    x[i] += d
                    resid -= d
                elif resid < 0 and x[i] > 0:
                    d = min(x[i], -resid)
                    x[i] -= d
                    resid += d
                if abs(resid) <= 1e-12:
                    break
        return [max(0.0, min(c, xi)) for xi, c in zip(x, caps)]

    @classmethod
    def bounded_weight_projection(
        cls,
        prev: Sequence[float],
        candidate: Sequence[float],
        caps: Sequence[float],
        delta_max: float = DELTA_W_MAX,
    ) -> List[float]:
        lo = [max(0.0, p - delta_max) for p in prev]
        hi = [min(c, p + delta_max) for p, c in zip(prev, caps)]

        target = 1.0 - sum(lo)
        cap_shift = [max(0.0, h - l) for h, l in zip(hi, lo)]

        if target < 0:
            target = 0.0
        if target > sum(cap_shift):
            # fallback: relax toward caps while staying close
            cap_shift = [max(0.0, c - l) for c, l in zip(caps, lo)]
            target = min(1.0 - sum(lo), sum(cap_shift))

        shifted_candidate = [c - l for c, l in zip(candidate, lo)]
        y = cls.project_capped_simplex(shifted_candidate, cap_shift, target=target)
        w = [l + yi for l, yi in zip(lo, y)]

        total = sum(w)
        if total != 0.0:
            w = [wi / total for wi in w]

        # hard safety finalization
        w = [max(0.0, min(c, wi)) for wi, c in zip(w, caps)]
        total = sum(w)
        if total > 0:
            w = [wi / total for wi in w]

        for i in range(len(w)):
            w[i] = min(max(w[i], max(0.0, prev[i] - delta_max)), min(caps[i], prev[i] + delta_max))

        # one last sum correction in feasible region
        resid = 1.0 - sum(w)
        for _ in range(10):
            if abs(resid) <= 1e-12:
                break
            for i in range(len(w)):
                lo_i = max(0.0, prev[i] - delta_max)
                hi_i = min(caps[i], prev[i] + delta_max)
                if resid > 0:
                    room = hi_i - w[i]
                    if room > 0:
                        d = min(room, resid)
                        w[i] += d
                        resid -= d
                else:
                    room = w[i] - lo_i
                    if room > 0:
                        d = min(room, -resid)
                        w[i] -= d
                        resid += d
                if abs(resid) <= 1e-12:
                    break

        return w

    def _adam_step(self, idx: int, grad: float) -> float:
        self.m[idx] = self.beta1 * self.m[idx] + (1.0 - self.beta1) * grad
        self.v[idx] = self.beta2 * self.v[idx] + (1.0 - self.beta2) * grad * grad
        mhat = self.m[idx] / (1.0 - self.beta1 ** self.t)
        vhat = self.v[idx] / (1.0 - self.beta2 ** self.t)
        return self.lr * mhat / (math.sqrt(vhat) + self.eps)

    def step_from_gradients(
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

        cand_w = []
        for i in range(self.nw):
            step = self._adam_step(i, grads[i])
            cand_w.append(weights[i] - step)

        new_w = self.bounded_weight_projection(weights, cand_w, caps, DELTA_W_MAX)

        fee_step = self._adam_step(self.nw, grads[-1])
        cand_fee = mint_fee - fee_step
        fee_low = max(0.0, mint_fee - DELTA_FEE_MAX)
        fee_high = min(0.02, mint_fee + DELTA_FEE_MAX)
        new_fee = min(max(cand_fee, fee_low), fee_high)
        return new_w, new_fee


# === Section 6: CircuitBreaker (state machine) ===


CB_NORMAL = "NORMAL"
CB_ACTIVE = "ACTIVE"
CB_COOLDOWN = "COOLDOWN"
CB_EXTENDED = "EXTENDED_ACTIVE"


@dataclass
class CBMachine:
    cb_id: int
    min_hold: int
    state: str = CB_NORMAL
    activated_tick: int = -10_000
    cooldown_until: int = -1
    recovery_streak: int = 0
    trigger_ticks: deque = field(default_factory=lambda: deque(maxlen=128))

    def is_active(self) -> bool:
        return self.state in (CB_ACTIVE, CB_EXTENDED)

    def effective_min_hold(self) -> int:
        return self.min_hold * (3 if self.state == CB_EXTENDED else 1)

    def can_trigger(self, tick: int) -> bool:
        if self.state == CB_COOLDOWN and tick < self.cooldown_until:
            return False
        return not self.is_active()

    def trigger(self, tick: int) -> bool:
        if not self.can_trigger(tick):
            return False
        self.trigger_ticks.append(tick)
        while self.trigger_ticks and self.trigger_ticks[0] < tick - 30:
            self.trigger_ticks.popleft()
        self.state = CB_EXTENDED if len(self.trigger_ticks) >= 3 else CB_ACTIVE
        self.activated_tick = tick
        self.recovery_streak = 0
        return True

    def hold_satisfied(self, tick: int) -> bool:
        return (tick - self.activated_tick) >= self.effective_min_hold()

    def to_cooldown(self, tick: int) -> None:
        self.state = CB_COOLDOWN
        self.cooldown_until = tick + 5
        self.recovery_streak = 0

    def update_cooldown(self, tick: int) -> None:
        if self.state == CB_COOLDOWN and tick >= self.cooldown_until:
            self.state = CB_NORMAL


class CircuitBreaker:
    """4-breaker state machine with priority and anti-zeno logic."""

    PRIORITY = [4, 3, 2, 1]

    def __init__(self):
        self.cb = {
            1: CBMachine(1, 5),
            2: CBMachine(2, 10),
            3: CBMachine(3, 3),
            4: CBMachine(4, 3),
        }
        self.depeg_streak = [0, 0, 0, 0]
        self.cb1_target_idx: Optional[int] = None
        self.cb1_recovery = 0
        self.cb2_recovery = 0
        self.cb3_recovery = 0
        self.cb4_decrease_streak = 0
        self.last_loss: Optional[float] = None
        self.last_applied_order: List[int] = []
        self.transition_log: List[Tuple[int, int, str, str]] = []

    @staticmethod
    def preview_transition(state: str, event: str) -> str:
        if state == CB_NORMAL and event == "trigger":
            return CB_ACTIVE
        if state == CB_ACTIVE and event == "recover":
            return CB_COOLDOWN
        if state == CB_EXTENDED and event == "recover":
            return CB_COOLDOWN
        if state == CB_COOLDOWN and event == "cooldown_done":
            return CB_NORMAL
        if state == CB_ACTIVE and event == "escalate":
            return CB_EXTENDED
        return state

    def is_active(self, cb_id: int) -> bool:
        return self.cb[cb_id].is_active()

    def _higher_active(self, cb_id: int) -> bool:
        for p in self.PRIORITY:
            if p < cb_id:
                continue
        for p in self.PRIORITY:
            if p > cb_id and self.cb[p].is_active():
                return True
        return False

    def _log_transition(self, tick: int, cb_id: int, old: str, new: str) -> None:
        if old != new:
            self.transition_log.append((tick, cb_id, old, new))

    def _trigger(self, tick: int, cb_id: int) -> None:
        m = self.cb[cb_id]
        old = m.state
        if m.trigger(tick):
            self._log_transition(tick, cb_id, old, m.state)

    def _try_recover(self, tick: int, cb_id: int, cond_ok: bool, needed_streak: int) -> None:
        m = self.cb[cb_id]
        if not m.is_active():
            return
        if self._higher_active(cb_id):
            return  # lower-level relaxations are deferred
        if cond_ok:
            m.recovery_streak += 1
        else:
            m.recovery_streak = 0

        if m.hold_satisfied(tick) and m.recovery_streak >= needed_streak:
            old = m.state
            m.to_cooldown(tick)
            self._log_transition(tick, cb_id, old, m.state)

    def _update_cooldowns(self, tick: int) -> None:
        for cb_id, m in self.cb.items():
            old = m.state
            m.update_cooldown(tick)
            self._log_transition(tick, cb_id, old, m.state)

    def _trigger_conditions(self, market: MarketTick, loss_finite: bool, loss_value: Optional[float], forced: Optional[Dict]) -> Dict[int, bool]:
        cond = {1: False, 2: False, 3: False, 4: False}

        for i, p in enumerate(market.prices):
            if abs(p - 1.0) > 0.02:
                self.depeg_streak[i] += 1
            else:
                self.depeg_streak[i] = 0

        idx = None
        for i, s in enumerate(self.depeg_streak):
            if s >= 3:
                idx = i
                cond[1] = True
                break
        if idx is not None:
            self.cb1_target_idx = idx

        depeg_count = sum(1 for p in market.prices if abs(p - 1.0) > 0.02)
        cond[2] = depeg_count >= 2

        cond[3] = market.stale_seconds > 120 or market.divergence > 0.02

        divergent = False
        if loss_value is not None and self.last_loss is not None and loss_value > self.last_loss * 1.5:
            divergent = True
        cond[4] = (not loss_finite) or divergent

        if forced:
            if forced.get("cb1"):
                cond[1] = True
                if "cb1_idx" in forced:
                    self.cb1_target_idx = int(forced["cb1_idx"])
            if forced.get("cb2"):
                cond[2] = True
            if forced.get("cb3"):
                cond[3] = True
            if forced.get("cb4"):
                cond[4] = True

        return cond

    def update(
        self,
        tick: int,
        state: ProtocolState,
        market: MarketTick,
        loss_finite: bool,
        loss_value: Optional[float],
        forced: Optional[Dict] = None,
    ) -> Dict[str, bool]:
        self.last_applied_order = []
        conditions = self._trigger_conditions(market, loss_finite, loss_value, forced)

        # trigger evaluation in priority order (higher first)
        for cb_id in self.PRIORITY:
            if conditions[cb_id]:
                self._trigger(tick, cb_id)

        # recovery conditions
        target_idx = self.cb1_target_idx if self.cb1_target_idx is not None else 0
        cb1_ok = abs(market.prices[target_idx] - 1.0) < 0.005
        cb2_ok = all(abs(p - 1.0) < 0.005 for p in market.prices)
        cb3_ok = market.stale_seconds <= 120 and market.divergence <= 0.02

        if loss_value is not None and self.last_loss is not None and loss_value < self.last_loss:
            self.cb4_decrease_streak += 1
        elif loss_value is not None:
            self.cb4_decrease_streak = 0

        self._try_recover(tick, 4, self.cb4_decrease_streak >= 3, 1)
        self._try_recover(tick, 3, cb3_ok, 5)
        self._try_recover(tick, 2, cb2_ok, 20)
        self._try_recover(tick, 1, cb1_ok, 10)

        self._update_cooldowns(tick)

        rollback = self.cb[4].is_active()

        # apply policy (worsening actions are immediate)
        state.reset_dynamic_policy()

        if self.cb[1].is_active() and self.cb1_target_idx is not None:
            i = self.cb1_target_idx
            state.w_caps[i] = min(state.w_caps[i], state.base_w_caps[i] * 0.5)
            state.mint_limit = min(state.mint_limit, 0.25)
            state.cr_target = max(state.cr_target, 1.25)
            self.last_applied_order.append(1)

        if self.cb[2].is_active():
            state.mint_limit = 0.0
            state.mint_paused_reason = "MINT_PAUSED_BY_CB2"
            state.cr_target = max(state.cr_target, 1.30)
            self.last_applied_order.append(2)

        if self.cb[3].is_active():
            state.optimizer_enabled = False
            state.conservative_mode = True
            state.oracle_degraded = True
            state.mint_limit = min(state.mint_limit, 0.10)
            state.cr_target = max(state.cr_target, 1.35)
            self.last_applied_order.append(3)

        if self.cb[4].is_active():
            self.last_applied_order.append(4)

        # Emergency compliance: if tightened caps are below current allocation, force-safe rebalance.
        if any(w > cap + 1e-12 for w, cap in zip(state.weights, state.w_caps)):
            new_w = [min(w, cap) for w, cap in zip(state.weights, state.w_caps)]
            resid = 1.0 - sum(new_w)
            room = [max(0.0, cap - w) for w, cap in zip(new_w, state.w_caps)]
            total_room = sum(room)
            if total_room > 0 and resid > 0:
                for i in range(len(new_w)):
                    add = resid * (room[i] / total_room)
                    new_w[i] += add
            s = sum(new_w)
            if s > 0:
                new_w = [w / s for w in new_w]
            state.weights = new_w
            state.prev_weights = new_w[:]

        if loss_value is not None and math.isfinite(loss_value):
            self.last_loss = loss_value

        return {
            "rollback": rollback,
            "cb1": self.cb[1].is_active(),
            "cb2": self.cb[2].is_active(),
            "cb3": self.cb[3].is_active(),
            "cb4": self.cb[4].is_active(),
        }


# === Section 7: AgentInterface (keeper/watchdog/auditor) ===


class Keeper:
    def propose(self, state: ProtocolState, optimizer: AdamBoundedOptimizer, grad_w: Sequence[float], grad_fee: float) -> Dict:
        new_w, new_fee = optimizer.step_from_gradients(state.weights, state.mint_fee, grad_w, grad_fee, state.w_caps)
        return {
            "weights": new_w,
            "mint_fee": new_fee,
            "status": "PROPOSED",
        }

    def submit_update_proposal(self, state: ProtocolState, proposal: Dict) -> Dict:
        w = proposal["weights"]
        if abs(sum(w) - 1.0) > 1e-6:
            return {"status": "REJECTED", "reason": "sum(weights)!=1"}
        for i, wi in enumerate(w):
            if wi < -1e-12 or wi > state.w_caps[i] + 1e-12:
                return {"status": "REJECTED", "reason": "out_of_bounds"}
            if abs(wi - state.weights[i]) > DELTA_W_MAX + 1e-9:
                return {"status": "REJECTED", "reason": "delta_cap"}

        fee = proposal.get("mint_fee", state.mint_fee)
        if abs(fee - state.mint_fee) > DELTA_FEE_MAX + 1e-12:
            return {"status": "REJECTED", "reason": "fee_delta_cap"}

        state.apply_weights_and_fee(w, fee)
        return {"status": "APPLIED", "weights": state.weights[:], "mint_fee": state.mint_fee}


class Watchdog:
    def detect(self, market: MarketTick) -> Dict[str, bool | int]:
        events: Dict[str, bool | int] = {}
        depeg_idx = None
        for i, p in enumerate(market.prices):
            if abs(p - 1.0) > 0.02:
                depeg_idx = i
                break
        if depeg_idx is not None:
            events["cb1"] = True
            events["cb1_idx"] = depeg_idx
        if sum(1 for p in market.prices if abs(p - 1.0) > 0.02) >= 2:
            events["cb2"] = True
        if market.stale_seconds > 120 or market.divergence > 0.02:
            events["cb3"] = True
        return events


class Auditor:
    def verify_invariants(self, state: ProtocolState) -> Dict:
        violations = []
        if abs(sum(state.weights) - 1.0) > 1e-6:
            violations.append("INV_WEIGHT_SUM")
        for i, (w, cap) in enumerate(zip(state.weights, state.w_caps)):
            if not (0.0 <= w <= cap + 1e-9):
                violations.append(f"INV_WEIGHT_BOUND_{i}")
        if state.cr < state.cr_hard_min:
            violations.append("INV_CR_HARD_MIN")
        for i, (wp, wn) in enumerate(zip(state.prev_weights, state.weights)):
            if abs(wn - wp) > DELTA_W_MAX + 1e-6:
                violations.append(f"INV_DELTA_CAP_{i}")
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


# === Section 8: Runner (scenario executor + metrics) ===


@dataclass
class ScenarioResult:
    scenario: str
    seed: int
    ticks: int
    peg_mae: float
    cr_violation_rate: float
    breaker_false_positive_rate: float
    cb_trigger_counts: Dict[int, int]
    cb_recovery_order_ok: bool
    invariant_violations: int
    cr_final: float
    cr_target_final: float
    cb1_triggered: bool
    cb1_recovered: bool
    cb1_recovery_time: int
    optimizer_disabled_ticks: int
    conservative_mode_seen: bool
    dynamic_updates_count: int
    max_weight_delta: float
    max_fee_delta: float


def assert_invariants(state: ProtocolState) -> None:
    assert abs(sum(state.weights) - 1.0) < 1e-6, "Weight sum != 1"
    assert all(0.0 <= w <= cap + 1e-9 for w, cap in zip(state.weights, state.w_caps)), "Weight cap violation"
    assert state.cr >= CR_HARD_MIN, f"CR {state.cr} < {CR_HARD_MIN}"
    for i, (wp, wn) in enumerate(zip(state.prev_weights, state.weights)):
        assert abs(wn - wp) <= DELTA_W_MAX + 1e-6, f"Delta cap violation {i}"


def run_scenario(scenario: str, seed: int = 0, ticks: int = 100, enforce_invariants: bool = True) -> ScenarioResult:
    env = MarketEnv(scenario=scenario, seed=seed)
    state = ProtocolState()
    loss_engine = LossEngine()
    optimizer = AdamBoundedOptimizer(n_weights=len(state.weights))
    cb = CircuitBreaker()
    keeper = Keeper()
    watchdog = Watchdog()

    checkpoint_state = state.clone()
    checkpoint_lr = optimizer.lr

    peg_errors: List[float] = []
    cr_violations = 0
    false_positives = 0
    inv_violations = 0
    trigger_counts = {1: 0, 2: 0, 3: 0, 4: 0}

    cb1_triggered = False
    cb1_recovered = False
    cb1_trigger_tick = -1
    cb1_recovery_time = -1
    optimizer_disabled_ticks = 0
    conservative_mode_seen = False
    dynamic_updates_count = 0
    max_weight_delta = 0.0
    max_fee_delta = 0.0

    for tick in range(ticks):
        state.begin_tick()
        old_fee = state.mint_fee

        market = env.step(tick)
        forced_events = watchdog.detect(market)

        loss_finite = True
        loss_value: Optional[float] = None
        grad_w = [0.0] * len(state.weights)
        grad_fee = 0.0

        try:
            loss, ctx = loss_engine.compute(state, market.prices, market.oracle_q)
            loss_value = loss.data
            loss.backward()
            grad_w = [wv.grad for wv in ctx["weights"]]  # type: ignore[index]
            grad_fee = ctx["fee"].grad  # type: ignore[index]
            if not all(math.isfinite(g) for g in grad_w + [grad_fee]):
                raise ValueError("non-finite gradient")
        except Exception:
            loss_finite = False

        cb_before = {i: cb.is_active(i) for i in range(1, 5)}
        actions = cb.update(tick, state, market, loss_finite=loss_finite, loss_value=loss_value, forced=forced_events)
        for i in range(1, 5):
            if cb.is_active(i) and not cb_before[i]:
                trigger_counts[i] += 1
        if cb.is_active(1) and not cb_before[1]:
            cb1_triggered = True
            if cb1_trigger_tick < 0:
                cb1_trigger_tick = tick
        if (not cb.is_active(1)) and cb_before[1] and cb1_trigger_tick >= 0 and cb1_recovery_time < 0:
            cb1_recovered = True
            cb1_recovery_time = tick - cb1_trigger_tick

        if actions["rollback"]:
            state = checkpoint_state.clone()
            optimizer.lr = checkpoint_lr * 0.5

        if state.optimizer_enabled and not actions["cb3"]:
            proposal = keeper.propose(state, optimizer, grad_w, grad_fee)
            apply_result = keeper.submit_update_proposal(state, proposal)
            if apply_result.get("status") == "APPLIED":
                dynamic_updates_count += 1
                for wp, wn in zip(state.prev_weights, state.weights):
                    max_weight_delta = max(max_weight_delta, abs(wn - wp))
                max_fee_delta = max(max_fee_delta, abs(state.mint_fee - old_fee))
        else:
            optimizer_disabled_ticks += 1

        peg_noise = 0.0
        if scenario == "volatile":
            peg_noise = env.rng.gauss(0.0, 0.00035)
        elif scenario == "normal":
            peg_noise = env.rng.gauss(0.0, 0.00012)
        else:
            peg_noise = env.rng.gauss(0.0, 0.00020)

        peg = state.update_market_state(market.prices, market.oracle_q, peg_noise=peg_noise)
        conservative_mode_seen = conservative_mode_seen or state.conservative_mode
        peg_errors.append(abs(peg - 1.0))

        if state.cr < state.cr_hard_min:
            cr_violations += 1

        no_real_stress = (
            sum(1 for p in market.prices if abs(p - 1.0) > 0.02) == 0
            and market.stale_seconds <= 120
            and market.divergence <= 0.02
            and loss_finite
        )
        if scenario == "normal" and any(cb.is_active(i) for i in (1, 2, 3, 4)) and no_real_stress:
            false_positives += 1

        try:
            if enforce_invariants:
                assert_invariants(state)
        except AssertionError:
            inv_violations += 1
            if enforce_invariants:
                raise

        checkpoint_state = state.clone()
        checkpoint_lr = optimizer.lr

    order_ok = True
    if cb.transition_log:
        # whenever recoveries happen among active breakers, ensure descending priority order
        recovered = [(t, cid) for (t, cid, old, new) in cb.transition_log if old in (CB_ACTIVE, CB_EXTENDED) and new == CB_COOLDOWN]
        if recovered:
            # sort by tick then by observed order; check cid non-increasing at same/next recovery events
            for i in range(1, len(recovered)):
                prev = recovered[i - 1][1]
                curr = recovered[i][1]
                if curr > prev and recovered[i][0] <= recovered[i - 1][0] + 1:
                    order_ok = False
                    break

    return ScenarioResult(
        scenario=scenario,
        seed=seed,
        ticks=ticks,
        peg_mae=mean(peg_errors) if peg_errors else 0.0,
        cr_violation_rate=cr_violations / float(ticks),
        breaker_false_positive_rate=false_positives / float(ticks),
        cb_trigger_counts=trigger_counts,
        cb_recovery_order_ok=order_ok,
        invariant_violations=inv_violations,
        cr_final=state.cr,
        cr_target_final=state.cr_target,
        cb1_triggered=cb1_triggered,
        cb1_recovered=cb1_recovered,
        cb1_recovery_time=cb1_recovery_time,
        optimizer_disabled_ticks=optimizer_disabled_ticks,
        conservative_mode_seen=conservative_mode_seen,
        dynamic_updates_count=dynamic_updates_count,
        max_weight_delta=max_weight_delta,
        max_fee_delta=max_fee_delta,
    )


def percentile(values: Sequence[float], p: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    if p <= 0:
        return s[0]
    if p >= 100:
        return s[-1]
    k = (len(s) - 1) * (p / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return s[int(k)]
    return s[f] * (c - k) + s[c] * (k - f)


def run_monte_carlo(scenarios: Sequence[str], seeds: int = 100, ticks: int = 100) -> Dict[str, Dict[str, float]]:
    out: Dict[str, Dict[str, float]] = {}
    for scenario in scenarios:
        results = [run_scenario(scenario, seed=s, ticks=ticks) for s in range(seeds)]
        peg = [r.peg_mae for r in results]
        crv = [r.cr_violation_rate for r in results]
        fpr = [r.breaker_false_positive_rate for r in results]
        out[scenario] = {
            "peg_mean": mean(peg),
            "peg_std": pstdev(peg),
            "peg_p95": percentile(peg, 95),
            "peg_worst": max(peg),
            "cr_p95": percentile(crv, 95),
            "fpr_p95": percentile(fpr, 95),
            "peg_p50": median(peg),
        }
    return out


# === Section 9: Tests (inline test suite) ===


def run_inline_smoke_tests() -> None:
    a = Value(2.0)
    b = Value(3.0)
    y = a * b + a
    y.backward()
    assert abs(y.data - 8.0) < 1e-12
    assert abs(a.grad - 4.0) < 1e-12
    assert abs(b.grad - 2.0) < 1e-12

    res = run_scenario("normal", seed=0, ticks=20)
    assert res.peg_mae < 0.003
    assert res.cr_violation_rate == 0.0


# === Section 10: Main (entry point) ===


def main() -> None:
    run_inline_smoke_tests()
    scenarios = ["normal", "single_depeg", "multi_depeg", "volatile", "gradient_attack", "oracle_failure"]
    print("microstable.py basic run")
    for sc in scenarios:
        r = run_scenario(sc, seed=0, ticks=120)
        print(
            f"{sc:15s} peg_mae={r.peg_mae:.6f} "
            f"cr_violation={r.cr_violation_rate:.4f} "
            f"fpr={r.breaker_false_positive_rate:.4f} "
            f"cb={r.cb_trigger_counts}"
        )


if __name__ == "__main__":
    main()
