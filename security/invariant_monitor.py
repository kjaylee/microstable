"""Runtime invariant monitor for microstable simulations."""

from __future__ import annotations

from collections import defaultdict, deque
from typing import Deque, Dict, Optional


class InvariantMonitor:
    def __init__(self) -> None:
        self._agent_actions: Dict[str, Deque[int]] = defaultdict(deque)
        self._agent_magnitude: Dict[str, Deque[float]] = defaultdict(deque)

    def record_agent_action(
        self,
        agent: str,
        tick: int,
        magnitude: float = 0.0,
        action_type: Optional[str] = None,
    ) -> None:
        key = f"{agent}:{action_type or 'generic'}"
        self._agent_actions[key].append(int(tick))
        self._agent_magnitude[key].append(float(magnitude))

    def _trim(self, tick: int, window_ticks: int) -> None:
        min_tick = tick - window_ticks
        for key in list(self._agent_actions.keys()):
            q = self._agent_actions[key]
            while q and q[0] < min_tick:
                q.popleft()
                self._agent_magnitude[key].popleft()

    def check(
        self,
        *,
        tick: int,
        state,
        market,
        weights,
        weight_caps,
        min_cr: float,
        oracle_stale_limit: int,
        max_actions_per_window: int,
        window_ticks: int,
    ) -> None:
        # // BLUE-TEAM: DEF-INV-CR - CR must never breach configured minimum.
        if float(state.cr) < float(min_cr) - 1e-9:
            raise AssertionError(f"INV_MONITOR_CR_BELOW_MIN tick={tick} cr={state.cr} min={min_cr}")

        # // BLUE-TEAM: DEF-INV-SUPPLY - accounting mirror must match total supply.
        if abs(float(state.supply) - float(state.position_supply_sum)) > 1e-6:
            raise AssertionError(
                f"INV_MONITOR_SUPPLY_MISMATCH tick={tick} supply={state.supply} positions={state.position_supply_sum}"
            )

        # // BLUE-TEAM: DEF-INV-WEIGHT - no collateral may exceed cap.
        for i, (w, cap) in enumerate(zip(weights, weight_caps)):
            if float(w) > float(cap) + 1e-10:
                raise AssertionError(f"INV_MONITOR_CAP_EXCEEDED tick={tick} idx={i} w={w} cap={cap}")

        # // BLUE-TEAM: DEF-INV-ORACLE - stale oracle data is forbidden.
        if int(market.stale_seconds) > int(oracle_stale_limit) and not bool(getattr(state, "oracle_degraded", False)):
            raise AssertionError(
                f"INV_MONITOR_ORACLE_STALE tick={tick} stale_seconds={market.stale_seconds}"
            )

        self._trim(tick, window_ticks)

        # // BLUE-TEAM: DEF-INV-RATE - detect excessive agent action bursts.
        for key, q in self._agent_actions.items():
            if len(q) > max_actions_per_window:
                raise AssertionError(
                    f"INV_MONITOR_AGENT_RATE_LIMIT key={key} count={len(q)} window={window_ticks}"
                )
