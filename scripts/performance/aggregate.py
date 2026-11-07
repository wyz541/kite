#!/usr/bin/env python3
"""Aggregate independent Kite performance reports into a noise-aware baseline."""

from __future__ import annotations

import argparse
import json
import math
import statistics
import sys
import time
from pathlib import Path
from typing import Any


def load_report(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        report = json.load(handle)
    if report.get("schema_version") != 1:
        raise ValueError(f"{path}: unsupported schema version")
    return report


def scenario_map(report: dict[str, Any]) -> dict[tuple[str, str], dict[str, Any]]:
    return {
        (result["area"], result["scenario"]): result
        for result in report.get("results", [])
        if result.get("status") == "measured" and result.get("summary")
    }


def median(values: list[float]) -> float:
    return float(statistics.median(values)) if values else 0.0


def spread_percent(values: list[float]) -> float:
    center = median(values)
    if center <= 0:
        return 0.0
    return (max(values) - min(values)) / center * 100.0


def classify_noise(spread: float, run_count: int) -> str:
    if run_count < 3:
        return "insufficient_runs"
    if spread <= 10.0:
        return "low"
    if spread <= 25.0:
        return "medium"
    return "high"


def recommended_threshold(spread: float, run_count: int) -> float:
    if run_count < 3:
        return 20.0
    return float(max(10, math.ceil(spread)))


def validate_reports(reports: list[dict[str, Any]], paths: list[Path]) -> list[str]:
    first = reports[0]
    mismatches: list[str] = []
    fields = [
        ("profile", lambda report: report.get("profile")),
        ("build_profile", lambda report: report.get("build_profile")),
        ("kite_version", lambda report: report.get("kite_version")),
        ("host.os", lambda report: report.get("host", {}).get("os")),
        ("host.architecture", lambda report: report.get("host", {}).get("architecture")),
        ("host.cpu_model", lambda report: report.get("host", {}).get("cpu_model")),
        ("git_commit", lambda report: report.get("git_commit")),
    ]
    for path, report in zip(paths[1:], reports[1:]):
        for name, getter in fields:
            if getter(report) != getter(first):
                mismatches.append(
                    f"{path.name}: {name}={getter(report)!r}, expected {getter(first)!r}"
                )
    if mismatches:
        raise ValueError("reports are not directly comparable:\n" + "\n".join(mismatches))
    return []


def aggregate_scenario(
    key: tuple[str, str],
    results: list[dict[str, Any]],
    run_count: int,
) -> dict[str, Any]:
    summaries = [result["summary"] for result in results]
    p50_values = [float(summary.get("p50_ms", 0.0)) for summary in summaries]
    p95_values = [float(summary.get("p95_ms", 0.0)) for summary in summaries]
    max_values = [float(summary.get("max_ms", 0.0)) for summary in summaries]
    mean_values = [float(summary.get("mean_ms", 0.0)) for summary in summaries]
    min_values = [float(summary.get("min_ms", 0.0)) for summary in summaries]
    rss_values = [
        int(summary.get("peak_rss_delta_bytes_max", 0)) for summary in summaries
    ]
    cpu_values = [int(summary.get("cpu_ms_p50", 0)) for summary in summaries]
    throughput_values = [
        float(summary["throughput_p50"])
        for summary in summaries
        if summary.get("throughput_p50") is not None
    ]
    through_units = {
        str(summary["throughput_unit"])
        for summary in summaries
        if summary.get("throughput_unit")
    }
    p95_spread = spread_percent(p95_values)
    first = results[0]
    summary = {
        "sample_count": min(int(item.get("sample_count", 0)) for item in summaries),
        "run_count": run_count,
        "min_ms": min(min_values),
        "p50_ms": median(p50_values),
        "p95_ms": median(p95_values),
        "max_ms": max(max_values),
        "mean_ms": median(mean_values),
        "throughput_p50": median(throughput_values) if throughput_values else None,
        "throughput_unit": next(iter(through_units)) if len(through_units) == 1 else None,
        "peak_rss_delta_bytes_max": max(rss_values),
        "cpu_ms_p50": int(median([float(value) for value in cpu_values])),
    }
    return {
        "area": key[0],
        "scenario": key[1],
        "boundary": first.get("boundary", ""),
        "status": "measured",
        "unit": "ms",
        "samples": [],
        "summary": summary,
        "details": {
            "run_p50_ms": p50_values,
            "run_p95_ms": p95_values,
            "run_max_ms": max_values,
            "run_peak_rss_delta_bytes": rss_values,
        },
        "noise": {
            "run_count": run_count,
            "p95_spread_percent": p95_spread,
            "classification": classify_noise(p95_spread, run_count),
            "recommended_comparison_threshold_pct": recommended_threshold(
                p95_spread, run_count
            ),
        },
        "notes": [f"Aggregated across {run_count} independent runs."],
    }


def build_aggregate(paths: list[Path], minimum_runs: int) -> dict[str, Any]:
    reports = [load_report(path) for path in paths]
    if len(reports) < minimum_runs:
        raise ValueError(
            f"received {len(reports)} runs; minimum is {minimum_runs}"
        )
    validate_reports(reports, paths)
    scenario_results: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for report in reports:
        for key, result in scenario_map(report).items():
            scenario_results.setdefault(key, []).append(result)

    run_count = len(reports)
    results = [
        aggregate_scenario(key, values, run_count)
        for key, values in sorted(scenario_results.items())
        if len(values) == run_count
    ]
    missing_scenarios = [
        {"area": key[0], "scenario": key[1], "runs": len(values)}
        for key, values in sorted(scenario_results.items())
        if len(values) != run_count
    ]
    first = reports[0]
    noisy = sorted(
        results,
        key=lambda result: result["noise"]["p95_spread_percent"],
        reverse=True,
    )
    slowest = sorted(
        results,
        key=lambda result: result["summary"]["p95_ms"],
        reverse=True,
    )
    coverage = first.get("coverage", [])
    not_automated = len(
        [result for result in first.get("results", []) if result.get("status") == "not_automated"]
    )
    return {
        "schema_version": 1,
        "generated_at_unix_ms": time.time_ns() // 1_000_000,
        "kite_version": first.get("kite_version"),
        "git_commit": first.get("git_commit"),
        "profile": first.get("profile"),
        "build_profile": first.get("build_profile"),
        "host": first.get("host"),
        "method": {
            **first.get("method", {}),
            "aggregation": "median of per-run p50 and p95 values",
            "noise_spread": "(max run p95 - min run p95) / median run p95",
            "minimum_runs": minimum_runs,
        },
        "fixtures": first.get("fixtures", []),
        "coverage": coverage,
        "summary": {
            "measured_scenarios": len(results),
            "not_automated_scenarios": not_automated,
            "errors": 0,
            "slowest_scenarios": [
                {
                    "area": result["area"],
                    "scenario": result["scenario"],
                    "p95_ms": result["summary"]["p95_ms"],
                    "sample_count": result["summary"]["sample_count"],
                }
                for result in slowest[:12]
            ],
            "highest_variance_scenarios": [
                {
                    "area": result["area"],
                    "scenario": result["scenario"],
                    "p95_to_p50_ratio": (
                        result["summary"]["p95_ms"] / result["summary"]["p50_ms"]
                        if result["summary"]["p50_ms"] > 0
                        else 0.0
                    ),
                    "sample_count": result["summary"]["sample_count"],
                }
                for result in noisy[:8]
            ],
            "highest_memory_scenarios": [
                {
                    "area": result["area"],
                    "scenario": result["scenario"],
                    "peak_rss_delta_bytes": result["summary"][
                        "peak_rss_delta_bytes_max"
                    ],
                    "sample_count": result["summary"]["sample_count"],
                }
                for result in sorted(
                    results,
                    key=lambda result: result["summary"][
                        "peak_rss_delta_bytes_max"
                    ],
                    reverse=True,
                )[:8]
            ],
        },
        "aggregation": {
            "run_count": run_count,
            "minimum_runs": minimum_runs,
            "source_reports": [path.name for path in paths],
            "missing_scenarios": missing_scenarios,
            "noise_counts": {
                level: len(
                    [
                        result
                        for result in results
                        if result["noise"]["classification"] == level
                    ]
                )
                for level in ("low", "medium", "high", "insufficient_runs")
            },
        },
        "results": results,
    }


def escape_cell(value: str) -> str:
    return value.replace("|", "\\|")


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Kite aggregated performance baseline",
        "",
        f"- Version: `{report['kite_version']}`",
        f"- Commit: `{report['git_commit']}`",
        f"- Profile: `{report['profile']}`",
        f"- Build: `{report['build_profile']}`",
        f"- Independent runs: `{report['aggregation']['run_count']}`",
        f"- Minimum runs: `{report['aggregation']['minimum_runs']}`",
        "",
        "## Noise summary",
        "",
        "| Noise | Scenarios |",
        "| --- | ---: |",
    ]
    for level in ("low", "medium", "high", "insufficient_runs"):
        lines.append(
            f"| {level} | {report['aggregation']['noise_counts'][level]} |"
        )
    lines.extend(
        [
            "",
            "Per-run p95 spread is the range divided by the median. The suggested comparison threshold is the larger of 10% and the observed p95 spread.",
            "",
            "## Results",
            "",
            "| Area | Scenario | p50 median ms | p95 median ms | p95 spread | Noise | Suggested delta threshold | Peak RSS max |",
            "| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |",
        ]
    )
    for result in report["results"]:
        summary = result["summary"]
        noise = result["noise"]
        lines.append(
            "| {area} | {scenario} | {p50:.3f} | {p95:.3f} | {spread:.1f}% | {level} | {threshold:.1f}% | {rss:.2f} MiB |".format(
                area=escape_cell(result["area"]),
                scenario=escape_cell(result["scenario"]),
                p50=summary["p50_ms"],
                p95=summary["p95_ms"],
                spread=noise["p95_spread_percent"],
                level=noise["classification"],
                threshold=noise["recommended_comparison_threshold_pct"],
                rss=summary["peak_rss_delta_bytes_max"] / 1024 / 1024,
            )
        )
    high_noise = [
        result
        for result in report["results"]
        if result["noise"]["classification"] == "high"
    ]
    if high_noise:
        lines.extend(
            [
                "",
                "## High-noise scenarios",
                "",
                "| Area | Scenario | p95 spread | Suggested threshold | Recommended action |",
                "| --- | --- | ---: | ---: | --- |",
            ]
        )
        for result in high_noise:
            noise = result["noise"]
            lines.append(
                f"| {escape_cell(result['area'])} | {escape_cell(result['scenario'])} | "
                f"{noise['p95_spread_percent']:.1f}% | "
                f"{noise['recommended_comparison_threshold_pct']:.1f}% | "
                "Collect more runs before claiming a regression |"
            )

    lines.extend(
        [
            "",
            "## Source reports",
            "",
        ]
    )
    for name in report["aggregation"]["source_reports"]:
        lines.append(f"- `{name}`")
    if report["aggregation"]["missing_scenarios"]:
        lines.extend(["", "## Missing scenarios", ""])
        for scenario in report["aggregation"]["missing_scenarios"]:
            lines.append(
                f"- `{scenario['area']} / {scenario['scenario']}`: {scenario['runs']} runs"
            )
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--minimum-runs", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown", type=Path)
    args = parser.parse_args()
    if args.minimum_runs < 1:
        parser.error("--minimum-runs must be greater than zero")
    return args


def main() -> int:
    args = parse_args()
    try:
        report = build_aggregate(args.reports, args.minimum_runs)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"Kite aggregate: {args.output}")
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(render_markdown(report), encoding="utf-8")
        print(f"Kite aggregate summary: {args.markdown}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
