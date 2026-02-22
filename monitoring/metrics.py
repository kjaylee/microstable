from __future__ import annotations

import json
import math
import os
import resource
import subprocess
import sys
import time
from collections import deque
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Deque, Dict, Iterable, List, Optional

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import MAX_AUTOGRAD_DEPTH


@dataclass
class AgentHealth:
    name: str
    last_activity_tick: int = -1
    last_response_ms: float = 0.0
    total_probes: int = 0
    failed_probes: int = 0

    def record(self, tick: int, response_ms: float, ok: bool) -> None:
        self.total_probes += 1
        if ok:
            self.last_activity_tick = tick
            self.last_response_ms = response_ms
        else:
            self.failed_probes += 1

    def as_dict(self, current_tick: int) -> Dict[str, object]:
        silent_ticks = current_tick - self.last_activity_tick if self.last_activity_tick >= 0 else current_tick + 1
        status = "healthy"
        if silent_ticks > 5:
            status = "warning"
        if silent_ticks > 10:
            status = "critical"
        return {
            "name": self.name,
            "status": status,
            "last_activity_tick": self.last_activity_tick,
            "silent_ticks": silent_ticks,
            "last_response_ms": round(self.last_response_ms, 3),
            "total_probes": self.total_probes,
            "failed_probes": self.failed_probes,
            "failure_rate": (self.failed_probes / self.total_probes) if self.total_probes else 0.0,
        }


@dataclass
class MetricsCollector:
    peg_errors: List[float] = field(default_factory=list)
    cr_values: List[float] = field(default_factory=list)
    tx_total: int = 0
    tx_failures: int = 0
    cb_activation_counts: Dict[int, int] = field(default_factory=lambda: {1: 0, 2: 0, 3: 0, 4: 0})
    cb_last: Dict[int, bool] = field(default_factory=lambda: {1: False, 2: False, 3: False, 4: False})
    cb_consecutive_active_ticks: int = 0
    cb_consecutive_peak: int = 0
    oracle_stale_streak: int = 0
    oracle_stale_peak: int = 0
    oracle_last_fresh_tick: int = -1
    tick_durations: Deque[float] = field(default_factory=lambda: deque(maxlen=2048))
    agent_health: Dict[str, AgentHealth] = field(
        default_factory=lambda: {
            "keeper": AgentHealth("keeper"),
            "watchdog": AgentHealth("watchdog"),
            "auditor": AgentHealth("auditor"),
        }
    )
    started_at: float = field(default_factory=time.time)
    current_tick: int = -1

    def record_tick(
        self,
        *,
        tick: int,
        peg: float,
        cr: float,
        cb_flags: Dict[int, bool],
        tx_success: bool,
        tick_duration_seconds: float,
        oracle_is_stale: bool,
    ) -> None:
        self.current_tick = max(self.current_tick, tick)
        err = abs(float(peg) - 1.0)
        self.peg_errors.append(err)
        self.cr_values.append(float(cr))

        self.tx_total += 1
        if not tx_success:
            self.tx_failures += 1

        any_active = False
        for cb_id in (1, 2, 3, 4):
            active = bool(cb_flags.get(cb_id, False))
            any_active = any_active or active
            if active and not self.cb_last[cb_id]:
                self.cb_activation_counts[cb_id] += 1
            self.cb_last[cb_id] = active

        if any_active:
            self.cb_consecutive_active_ticks += 1
            self.cb_consecutive_peak = max(self.cb_consecutive_peak, self.cb_consecutive_active_ticks)
        else:
            self.cb_consecutive_active_ticks = 0

        if oracle_is_stale:
            self.oracle_stale_streak += 1
            self.oracle_stale_peak = max(self.oracle_stale_peak, self.oracle_stale_streak)
        else:
            self.oracle_stale_streak = 0
            self.oracle_last_fresh_tick = tick

        self.tick_durations.append(max(1e-9, float(tick_duration_seconds)))

    def record_agent_probe(self, name: str, tick: int, response_ms: float, ok: bool) -> None:
        if name not in self.agent_health:
            self.agent_health[name] = AgentHealth(name)
        self.agent_health[name].record(tick=tick, response_ms=response_ms, ok=ok)

    @staticmethod
    def _memory_bytes() -> int:
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        # macOS returns bytes, Linux returns KiB.
        if rss < 10_000_000:
            return int(rss * 1024)
        return int(rss)

    @staticmethod
    def _cpu_usage_ratio() -> float:
        try:
            load1, _, _ = os.getloadavg()
            cpus = max(1, os.cpu_count() or 1)
            return max(0.0, min(1.0, load1 / cpus))
        except Exception:
            return 0.0

    def summary(self) -> Dict[str, object]:
        now = time.time()
        elapsed = max(1e-9, now - self.started_at)
        mae = sum(self.peg_errors) / len(self.peg_errors) if self.peg_errors else 0.0
        max_err = max(self.peg_errors) if self.peg_errors else 0.0
        cr_min = min(self.cr_values) if self.cr_values else 0.0
        cr_max = max(self.cr_values) if self.cr_values else 0.0
        cr_avg = (sum(self.cr_values) / len(self.cr_values)) if self.cr_values else 0.0
        cr_now = self.cr_values[-1] if self.cr_values else 0.0

        throughput = self.tx_total / elapsed
        failure_rate = self.tx_failures / self.tx_total if self.tx_total else 0.0

        return {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "tick": self.current_tick,
            "peg": {
                "mae": mae,
                "max_error": max_err,
                "samples": len(self.peg_errors),
            },
            "collateral_ratio": {
                "current": cr_now,
                "min": cr_min,
                "max": cr_max,
                "avg": cr_avg,
            },
            "circuit_breaker": {
                "active": [cb for cb in (1, 2, 3, 4) if self.cb_last[cb]],
                "activation_counts": self.cb_activation_counts,
                "consecutive_active_ticks": self.cb_consecutive_active_ticks,
                "consecutive_active_peak": self.cb_consecutive_peak,
            },
            "agent_health": {
                name: item.as_dict(self.current_tick)
                for name, item in self.agent_health.items()
            },
            "transactions": {
                "throughput_per_sec": throughput,
                "total": self.tx_total,
                "failures": self.tx_failures,
                "failure_rate": failure_rate,
            },
            "system": {
                "memory_bytes": self._memory_bytes(),
                "cpu_usage_ratio": self._cpu_usage_ratio(),
                "autograd_depth_cap": MAX_AUTOGRAD_DEPTH,
            },
            "oracle": {
                "stale_streak_ticks": self.oracle_stale_streak,
                "stale_peak_ticks": self.oracle_stale_peak,
                "last_fresh_tick": self.oracle_last_fresh_tick,
                "is_stale": self.oracle_stale_streak > 0,
            },
        }

    def to_json(self) -> str:
        return json.dumps(self.summary(), indent=2, sort_keys=True)


def probe_agents(tick: int, timeout_sec: float = 3.0) -> Dict[str, Dict[str, object]]:
    probes = {
        "keeper": ["python3", "agents/keeper.py", "--dry-run", "--tick", str(tick)],
        "watchdog": ["python3", "agents/watchdog.py", "--dry-run", "--tick", str(tick)],
        "auditor": ["python3", "agents/auditor.py", "--dry-run", "--round-id", str(tick), "--state-hash", "health"],
    }

    out: Dict[str, Dict[str, object]] = {}
    for name, cmd in probes.items():
        t0 = time.perf_counter()
        ok = False
        err = ""
        try:
            subprocess.run(cmd, cwd=ROOT_DIR, check=True, capture_output=True, text=True, timeout=timeout_sec)
            ok = True
        except Exception as exc:
            err = str(exc)
        dt_ms = (time.perf_counter() - t0) * 1000.0
        out[name] = {
            "ok": ok,
            "response_ms": dt_ms,
            "error": err,
        }
    return out


def replay_rows(rows: Iterable[Dict[str, object]], tick_seconds: float = 1.0) -> Dict[str, object]:
    collector = MetricsCollector()
    for row in rows:
        tick = int(row.get("tick", 0))
        peg = float(row.get("peg", 1.0))
        cr = float(row.get("cr", 0.0))
        cb_flags = {
            1: bool(int(row.get("cb1", 0))),
            2: bool(int(row.get("cb2", 0))),
            3: bool(int(row.get("cb3", 0))),
            4: bool(int(row.get("cb4", 0))),
        }
        oracle_q = float(row.get("oracle_q", 1.0))
        collector.record_tick(
            tick=tick,
            peg=peg,
            cr=cr,
            cb_flags=cb_flags,
            tx_success=bool(int(row.get("optimizer_enabled", 1))),
            tick_duration_seconds=tick_seconds,
            oracle_is_stale=oracle_q < 0.70,
        )
    return collector.summary()


if __name__ == "__main__":
    from microstable import run_scenario

    summary = run_scenario("volatile", ticks=90)
    print(json.dumps(replay_rows(summary.rows), indent=2, sort_keys=True))
