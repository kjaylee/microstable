from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from typing import Dict, List, Optional

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def build_dashboard_payload(
    metrics: Dict[str, object],
    alerts: List[Dict[str, object]],
    healthcheck: Optional[Dict[str, object]] = None,
    scenario: str = "live",
) -> Dict[str, object]:
    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scenario": scenario,
        "metrics": metrics,
        "alerts": alerts,
        "healthcheck": healthcheck or {"status": "unknown", "checks": []},
    }


def export_dashboard_json(payload: Dict[str, object], output_path: str) -> str:
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, sort_keys=True)
    return output_path


def export_to_default_docs(
    metrics: Dict[str, object],
    alerts: List[Dict[str, object]],
    healthcheck: Optional[Dict[str, object]] = None,
    scenario: str = "live",
) -> str:
    payload = build_dashboard_payload(metrics=metrics, alerts=alerts, healthcheck=healthcheck, scenario=scenario)
    out = os.path.join(ROOT_DIR, "docs", "dashboard-data.json")
    return export_dashboard_json(payload, out)
