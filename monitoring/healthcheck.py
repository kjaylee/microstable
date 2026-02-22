from __future__ import annotations

import json
import math
import os
import resource
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from typing import Dict, List, Optional

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from microstable import MAX_AUTOGRAD_DEPTH, ProtocolState


def _memory_bytes() -> int:
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if rss < 10_000_000:
        return int(rss * 1024)
    return int(rss)


def _check_oracle_connectivity(rpc_url: str) -> Dict[str, object]:
    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth",
            "params": [],
        }
    ).encode("utf-8")
    req = urllib.request.Request(rpc_url, data=payload, headers={"Content-Type": "application/json"}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=2.5) as resp:
            body = resp.read().decode("utf-8", errors="replace")
        ok = '"ok"' in body.lower()
        return {"ok": ok, "detail": body[:180]}
    except urllib.error.HTTPError as exc:
        return {"ok": False, "detail": f"HTTP {exc.code}: {exc.reason}"}
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "detail": str(exc)}


def run_healthcheck(
    *,
    state: Optional[ProtocolState] = None,
    rows: Optional[List[Dict[str, object]]] = None,
    agent_status: Optional[Dict[str, bool]] = None,
    solana_rpc_url: Optional[str] = None,
    max_memory_bytes: int = 1_500_000_000,
) -> Dict[str, object]:
    st = state or ProtocolState()
    checks: List[Dict[str, object]] = []

    def add_check(name: str, ok: bool, severity: str, detail: str) -> None:
        checks.append(
            {
                "name": name,
                "ok": bool(ok),
                "severity": severity,
                "detail": detail,
            }
        )

    # 1) Protocol state validity
    weight_sum = sum(st.weights)
    add_check(
        "weights_sum",
        abs(weight_sum - 1.0) <= 1e-6,
        "critical",
        f"sum(weights)={weight_sum:.8f}",
    )

    cr_ok = st.cr_hard_min <= st.cr <= 3.0 and st.cr >= st.cr_target - 1e-6
    add_check(
        "cr_bounds",
        cr_ok,
        "critical",
        f"cr={st.cr:.6f}, target={st.cr_target:.6f}, hard_min={st.cr_hard_min:.6f}",
    )

    supply_ok = (
        st.supply > 0.0
        and math.isfinite(st.supply)
        and math.isfinite(st.position_supply_sum)
        and abs(st.supply - st.position_supply_sum) <= 1e-6
    )
    add_check(
        "supply_consistency",
        supply_ok,
        "critical",
        f"supply={st.supply:.4f}, position_supply_sum={st.position_supply_sum:.4f}",
    )

    # 2) Agent quorum availability
    status = agent_status or {"keeper": True, "watchdog": True, "auditor": True}
    alive = [name for name, ok in status.items() if ok]
    quorum_ok = len(alive) >= 2
    add_check(
        "agent_quorum",
        quorum_ok,
        "critical",
        f"alive={alive} ({len(alive)}/3)",
    )

    # 3) Oracle connection + freshness
    stale_streak = 0
    if rows:
        for row in reversed(rows):
            q = float(row.get("oracle_q", 1.0))
            if q < 0.70:
                stale_streak += 1
            else:
                break
    add_check(
        "oracle_freshness",
        stale_streak <= 10,
        "warning",
        f"stale_streak_ticks={stale_streak}",
    )

    # 4) Solana RPC connectivity
    rpc_url = solana_rpc_url or os.getenv("SOLANA_RPC_URL", "https://api.devnet.solana.com")
    rpc = _check_oracle_connectivity(rpc_url)
    add_check(
        "solana_rpc",
        bool(rpc.get("ok", False)),
        "warning",
        f"url={rpc_url}, detail={rpc.get('detail', '')}",
    )

    # 5) Memory + graph depth safety
    mem = _memory_bytes()
    add_check(
        "memory_limit",
        mem <= max_memory_bytes,
        "warning",
        f"rss={mem} bytes",
    )

    add_check(
        "autograd_depth_cap",
        MAX_AUTOGRAD_DEPTH <= 512,
        "warning",
        f"MAX_AUTOGRAD_DEPTH={MAX_AUTOGRAD_DEPTH}",
    )

    critical_failed = any((not c["ok"]) and c["severity"] == "critical" for c in checks)
    warning_failed = any((not c["ok"]) and c["severity"] == "warning" for c in checks)

    if critical_failed:
        status_str = "critical"
    elif warning_failed:
        status_str = "degraded"
    else:
        status_str = "healthy"

    return {
        "status": status_str,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "checks": checks,
    }


if __name__ == "__main__":
    print(json.dumps(run_healthcheck(), indent=2, sort_keys=True))
