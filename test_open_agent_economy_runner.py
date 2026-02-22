#!/usr/bin/env python3
"""Mega test runner for Open Agent Economy (multiprocessing)."""
from __future__ import annotations

import json
import multiprocessing as mp
import os
import time
from typing import Dict, List

import open_agent_economy as oae
import test_open_agent_economy as tests

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "outputs")
REPORT_MD = os.path.join(OUTPUT_DIR, "open-agent-economy-test-report.md")
REPORT_JSON = os.path.join(OUTPUT_DIR, "open-agent-economy-test-results.json")


def _run_category(category: str) -> List[Dict[str, str]]:
    return tests.run_category(category)


def main() -> None:
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    categories = tests.CATEGORY_NAMES

    start = time.time()
    ctx = mp.get_context("spawn")
    workers = min(len(categories), max(1, mp.cpu_count() - 1))
    with ctx.Pool(processes=workers) as pool:
        results_per_category = pool.map(_run_category, categories)

    results = [item for sub in results_per_category for item in sub]
    total = len(results)
    passed = sum(1 for r in results if r["status"] == "PASS")
    failed = sum(1 for r in results if r["status"] == "FAIL")
    skipped = sum(1 for r in results if r["status"] == "SKIP")

    # Category summary
    category_summary: Dict[str, Dict[str, int]] = {}
    for r in results:
        cat = r["category"]
        if cat not in category_summary:
            category_summary[cat] = {"PASS": 0, "FAIL": 0, "SKIP": 0}
        category_summary[cat][r["status"]] += 1

    mc_stats = oae.run_monte_carlo_suite(seed=0, runs=100)

    elapsed = time.time() - start

    # Write JSON
    with open(REPORT_JSON, "w", encoding="utf-8") as f:
        json.dump(
            {
                "total": total,
                "passed": passed,
                "failed": failed,
                "skipped": skipped,
                "elapsed_sec": elapsed,
                "category_summary": category_summary,
                "monte_carlo": mc_stats,
                "results": results,
            },
            f,
            indent=2,
        )

    # Write Markdown report
    lines = []
    lines.append("# Open Agent Economy Test Report\n")
    lines.append(f"- Total: {total}")
    lines.append(f"- PASS: {passed}")
    lines.append(f"- FAIL: {failed}")
    lines.append(f"- SKIP: {skipped}")
    lines.append(f"- Elapsed: {elapsed:.2f}s\n")

    lines.append("## Category Summary\n")
    lines.append("| Category | PASS | FAIL | SKIP |")
    lines.append("|---|---:|---:|---:|")
    for cat, d in category_summary.items():
        lines.append(f"| {cat} | {d['PASS']} | {d['FAIL']} | {d['SKIP']} |")

    lines.append("\n## Monte Carlo Stats\n")
    for k, v in mc_stats.items():
        lines.append(f"- **{k}**: mean={v['mean']:.6f}, median={v['median']:.6f}, p5={v['p5']:.6f}, p95={v['p95']:.6f}, worst={v['worst']:.6f}")

    lines.append("\n## Failures\n")
    failures = [r for r in results if r["status"] == "FAIL"]
    if not failures:
        lines.append("- None")
    else:
        for r in failures:
            lines.append(f"- {r['id']} ({r['category']}): {r['detail']}")

    with open(REPORT_MD, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print("\n".join(lines))
    print("\nOutputs:")
    print(f"- {REPORT_MD}")
    print(f"- {REPORT_JSON}")


if __name__ == "__main__":
    main()
