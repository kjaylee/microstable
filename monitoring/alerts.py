from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List


@dataclass
class Alert:
    severity: str
    code: str
    message: str
    value: float
    threshold: float

    def as_dict(self) -> Dict[str, object]:
        return {
            "severity": self.severity,
            "code": self.code,
            "message": self.message,
            "value": self.value,
            "threshold": self.threshold,
        }


class AlertEngine:
    """Operational alert rules for Microstable observability."""

    def evaluate(self, metrics: Dict[str, object]) -> List[Alert]:
        out: List[Alert] = []

        peg = metrics.get("peg", {}) if isinstance(metrics, dict) else {}
        cr = metrics.get("collateral_ratio", {}) if isinstance(metrics, dict) else {}
        oracle = metrics.get("oracle", {}) if isinstance(metrics, dict) else {}
        system = metrics.get("system", {}) if isinstance(metrics, dict) else {}
        cb = metrics.get("circuit_breaker", {}) if isinstance(metrics, dict) else {}
        agents = metrics.get("agent_health", {}) if isinstance(metrics, dict) else {}

        peg_max_error = float(peg.get("max_error", 0.0))
        if peg_max_error > 0.02:
            out.append(
                Alert(
                    severity="CRITICAL",
                    code="PEG_DEVIATION",
                    message="Peg deviation exceeded 2%",
                    value=peg_max_error,
                    threshold=0.02,
                )
            )

        cr_current = float(cr.get("current", 0.0))
        if cr_current < 1.10:
            out.append(
                Alert(
                    severity="CRITICAL",
                    code="CR_BELOW_110",
                    message="Collateral ratio dropped below 110%",
                    value=cr_current,
                    threshold=1.10,
                )
            )

        for name, info in (agents.items() if isinstance(agents, dict) else []):
            silent_ticks = float(info.get("silent_ticks", 0.0)) if isinstance(info, dict) else 0.0
            if silent_ticks > 5:
                out.append(
                    Alert(
                        severity="WARNING",
                        code=f"AGENT_UNRESPONSIVE_{name.upper()}",
                        message=f"Agent {name} unresponsive for more than 5 ticks",
                        value=silent_ticks,
                        threshold=5.0,
                    )
                )

        stale_streak = float(oracle.get("stale_streak_ticks", 0.0))
        if stale_streak > 10:
            out.append(
                Alert(
                    severity="WARNING",
                    code="ORACLE_STALE",
                    message="Oracle stale streak exceeded 10 ticks",
                    value=stale_streak,
                    threshold=10.0,
                )
            )

        memory_bytes = float(system.get("memory_bytes", 0.0))
        if memory_bytes > 1_000_000_000:
            out.append(
                Alert(
                    severity="WARNING",
                    code="MEMORY_PRESSURE",
                    message="Memory usage exceeded 1GB",
                    value=memory_bytes,
                    threshold=1_000_000_000,
                )
            )

        cb_consecutive = float(cb.get("consecutive_active_ticks", 0.0))
        if cb_consecutive > 3:
            out.append(
                Alert(
                    severity="CRITICAL",
                    code="CB_CONSECUTIVE_ACTIVATION",
                    message="Circuit breaker active for more than 3 consecutive ticks",
                    value=cb_consecutive,
                    threshold=3.0,
                )
            )

        return out


def alerts_to_dict(alerts: List[Alert]) -> List[Dict[str, object]]:
    return [a.as_dict() for a in alerts]
