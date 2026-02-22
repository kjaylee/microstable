from __future__ import annotations

import json
import os
import sys
from datetime import datetime, timezone
from typing import Dict, List

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from chaos.scenarios import ScenarioResult, run_all_chaos_scenarios
from monitoring.alerts import AlertEngine, alerts_to_dict
from monitoring.dashboard_data import export_dashboard_json
from monitoring.healthcheck import run_healthcheck


def _ensure_dir(path: str) -> None:
    os.makedirs(path, exist_ok=True)


def _write_json(path: str, obj: Dict[str, object]) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2, sort_keys=True)


def _write_trace_jsonl(path: str, trace: List[Dict[str, object]]) -> None:
    with open(path, "w", encoding="utf-8") as f:
        for row in trace:
            f.write(json.dumps(row, sort_keys=True) + "\n")


def _scenario_to_payload(result: ScenarioResult) -> Dict[str, object]:
    return {
        "name": result.name,
        "passed": result.passed,
        "recovery_ticks": result.recovery_ticks,
        "impact_scope": result.impact_scope,
        "details": {
            "reason": result.details.get("reason"),
            "extra": result.details.get("extra"),
        },
    }


def _summary_markdown(results: List[ScenarioResult], out_dir: str) -> str:
    lines: List[str] = []
    lines.append("# Chaos Engineering Run Summary")
    lines.append("")
    lines.append(f"- generated_at: {datetime.now(timezone.utc).isoformat()}")
    lines.append(f"- total_scenarios: {len(results)}")
    lines.append(f"- pass: {sum(1 for r in results if r.passed)}")
    lines.append(f"- fail: {sum(1 for r in results if not r.passed)}")
    lines.append("")
    lines.append("| Scenario | Status | Recovery (ticks) | Max Peg Error | Min CR | Max CB |")
    lines.append("|---|---|---:|---:|---:|---:|")

    for r in results:
        imp = r.impact_scope
        lines.append(
            "| {name} | {status} | {recovery} | {peg:.4f} | {cr:.4f} | {cb} |".format(
                name=r.name,
                status="PASS" if r.passed else "FAIL",
                recovery=r.recovery_ticks,
                peg=float(imp.get("max_peg_error", 0.0)),
                cr=float(imp.get("min_cr", 0.0)),
                cb=int(imp.get("max_cb_level", 0)),
            )
        )

    lines.append("")
    lines.append("## Scenario Notes")
    lines.append("")
    for r in results:
        lines.append(f"### {r.name}")
        lines.append(f"- status: {'PASS' if r.passed else 'FAIL'}")
        lines.append(f"- reason: {r.details.get('reason')}")
        lines.append(f"- recovery_ticks: {r.recovery_ticks}")
        lines.append(f"- impact_scope: `{json.dumps(r.impact_scope, sort_keys=True)}`")
        lines.append("")

    path = os.path.join(out_dir, "chaos-summary.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines).strip() + "\n")
    return path


def run(output_dir: str | None = None) -> Dict[str, object]:
    out_dir = output_dir or os.path.join(ROOT_DIR, "outputs", "chaos")
    _ensure_dir(out_dir)

    results = run_all_chaos_scenarios()

    serialized: List[Dict[str, object]] = []
    all_rows: List[Dict[str, object]] = []
    for r in results:
        payload = _scenario_to_payload(r)
        serialized.append(payload)
        trace = list(r.details.get("trace", []))
        all_rows.extend(trace)

        _write_json(os.path.join(out_dir, f"{r.name}.json"), payload)
        _write_trace_jsonl(os.path.join(out_dir, f"{r.name}.trace.jsonl"), trace)

    # Build monitoring overlays from the final scenario's metrics.
    latest_metrics = {}
    if results:
        latest_metrics = dict(results[-1].details.get("extra", {}).get("metrics", {}))

    alerts = alerts_to_dict(AlertEngine().evaluate(latest_metrics)) if latest_metrics else []
    health = run_healthcheck(rows=all_rows)

    dashboard_payload = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "results": serialized,
        "latest_metrics": latest_metrics,
        "alerts": alerts,
        "healthcheck": health,
    }
    dashboard_path = os.path.join(out_dir, "dashboard-data.json")
    export_dashboard_json(dashboard_payload, dashboard_path)
    docs_dashboard_path = os.path.join(ROOT_DIR, "docs", "dashboard-data.json")
    export_dashboard_json(dashboard_payload, docs_dashboard_path)

    summary_path = _summary_markdown(results, out_dir)
    final_payload = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "output_dir": out_dir,
        "summary_path": summary_path,
        "dashboard_path": dashboard_path,
        "docs_dashboard_path": docs_dashboard_path,
        "results": serialized,
        "alerts": alerts,
        "healthcheck": health,
    }
    _write_json(os.path.join(out_dir, "chaos-results.json"), final_payload)
    return final_payload


if __name__ == "__main__":
    result = run()
    print(json.dumps(result, indent=2, sort_keys=True))
