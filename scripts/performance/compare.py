#!/usr/bin/env python3
"""Compare two Kite performance JSON reports."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_report(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        report = json.load(handle)
    if report.get("schema_version") != 1:
        raise ValueError(f"{path}: unsupported schema version")
    return report


def scenario_map(report: dict) -> dict[tuple[str, str], dict]:
    return {
        (result["area"], result["scenario"]): result
        for result in report.get("results", [])
        if result.get("status") == "measured" and result.get("summary")
    }


def change_percent(baseline: float, candidate: float) -> float | None:
    if baseline <= 0:
        return None
    return (candidate - baseline) / baseline * 100.0


def format_change(value: float | None) -> str:
    if value is None:
        return "-"
    return f"{value:+.1f}%"


def classify(value: float | None, threshold: float) -> str:
    if value is None:
        return "not_comparable"
    if value > threshold:
        return "above_threshold"
    if value < -threshold:
        return "below_threshold"
    return "stable"


def escape_cell(value: str) -> str:
    return value.replace("|", "\\|")


def build_rows(baseline: dict, candidate: dict, threshold: float) -> list[dict]:
    baseline_results = scenario_map(baseline)
    candidate_results = scenario_map(candidate)
    rows = []
    for key in sorted(set(baseline_results) & set(candidate_results)):
        before = baseline_results[key]
        after = candidate_results[key]
        before_summary = before["summary"]
        after_summary = after["summary"]
        p95_change = change_percent(
            float(before_summary.get("p95_ms", 0.0)),
            float(after_summary.get("p95_ms", 0.0)),
        )
        p50_change = change_percent(
            float(before_summary.get("p50_ms", 0.0)),
            float(after_summary.get("p50_ms", 0.0)),
        )
        rows.append(
            {
                "area": key[0],
                "scenario": key[1],
                "boundary": before.get("boundary", ""),
                "baseline_p50": float(before_summary.get("p50_ms", 0.0)),
                "candidate_p50": float(after_summary.get("p50_ms", 0.0)),
                "p50_change": p50_change,
                "baseline_p95": float(before_summary.get("p95_ms", 0.0)),
                "candidate_p95": float(after_summary.get("p95_ms", 0.0)),
                "p95_change": p95_change,
                "classification": classify(p95_change, threshold),
                "baseline_n": int(before_summary.get("sample_count", 0)),
                "candidate_n": int(after_summary.get("sample_count", 0)),
            }
        )
    rows.sort(
        key=lambda row: (
            row["p95_change"] is None,
            -(row["p95_change"] or 0.0),
            row["area"],
            row["scenario"],
        )
    )
    return rows


def render_markdown(baseline: dict, candidate: dict, rows: list[dict], threshold: float) -> str:
    lines = [
        "# Kite performance comparison",
        "",
        f"- Baseline: `{baseline.get('kite_version')}` / `{baseline.get('git_commit')}` / `{baseline.get('profile')}`",
        f"- Candidate: `{candidate.get('kite_version')}` / `{candidate.get('git_commit')}` / `{candidate.get('profile')}`",
        f"- Regression threshold: `{threshold:.1f}%` on p95",
        "",
    ]
    if baseline.get("profile") != candidate.get("profile"):
        lines.append(
            "- Warning: profiles differ. Treat cross-profile deltas as directional only."
        )
        lines.append("")
    if baseline.get("host", {}).get("os") != candidate.get("host", {}).get("os"):
        lines.append(
            "- Warning: operating systems differ. Compare platform-specific thresholds separately."
        )
        lines.append("")

    lines.extend(
        [
            "| Area | Scenario | Boundary | Baseline p95 ms | Candidate p95 ms | p95 delta | p50 delta | n | Result |",
            "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |",
        ]
    )
    for row in rows:
        lines.append(
            "| {area} | {scenario} | {boundary} | {baseline_p95:.3f} | {candidate_p95:.3f} | {p95} | {p50} | {n}/{m} | {result} |".format(
                area=escape_cell(row["area"]),
                scenario=escape_cell(row["scenario"]),
                boundary=escape_cell(row["boundary"]),
                baseline_p95=row["baseline_p95"],
                candidate_p95=row["candidate_p95"],
                p95=format_change(row["p95_change"]),
                p50=format_change(row["p50_change"]),
                n=row["baseline_n"],
                m=row["candidate_n"],
                result=row["classification"],
            )
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--threshold", type=float, default=20.0)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--fail-on-regression", action="store_true")
    args = parser.parse_args()

    baseline = load_report(args.baseline)
    candidate = load_report(args.candidate)
    if args.threshold < 0:
        raise SystemExit("--threshold must be zero or greater")

    rows = build_rows(baseline, candidate, args.threshold)
    markdown = render_markdown(baseline, candidate, rows, args.threshold)
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(markdown, encoding="utf-8")
    else:
        sys.stdout.write(markdown)

    above_threshold = [
        row for row in rows if row["classification"] == "above_threshold"
    ]
    compare_only = [
        row
        for row in rows
        if row["baseline_n"] < 10 or row["candidate_n"] < 10
    ]
    print(
        f"Compared {len(rows)} scenarios: {len(above_threshold)} above threshold, "
        f"{sum(row['classification'] == 'below_threshold' for row in rows)} below threshold, "
        f"{len(compare_only)} with fewer than 10 samples.",
        file=sys.stderr,
    )
    if args.fail_on_regression and above_threshold:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
