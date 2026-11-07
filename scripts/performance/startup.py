#!/usr/bin/env python3
"""Measure Kite release startup to first native content."""

from __future__ import annotations

import argparse
import ctypes
import json
import math
import os
import platform
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


TARGET_EVENTS = {
    "darwin": "macos.first_content",
    "win32": "windows.first_content_render",
}


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    if len(values) == 1:
        return values[0]
    ordered = sorted(values)
    position = (len(ordered) - 1) * min(max(fraction, 0.0), 1.0)
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def summarize(values: list[float]) -> dict[str, Any]:
    if not values:
        return {"sample_count": 0}
    ordered = sorted(values)
    return {
        "sample_count": len(values),
        "min_ms": ordered[0],
        "p50_ms": percentile(ordered, 0.50),
        "p95_ms": percentile(ordered, 0.95),
        "max_ms": ordered[-1],
        "mean_ms": statistics.fmean(values),
    }


def phase_summary(runs: list[dict[str, Any]]) -> dict[str, Any]:
    launch = [float(run["launch_to_first_content_ms"]) for run in runs]
    entry = [float(run["entry_to_first_content_ms"]) for run in runs]
    rss = [
        int(run["peak_rss_bytes"])
        for run in runs
        if run.get("peak_rss_bytes") is not None
    ]
    return {
        "launch_to_first_content": summarize(launch),
        "entry_to_first_content": summarize(entry),
        "peak_rss_bytes_max": max(rss) if rss else None,
    }


def create_fixture(root: Path, entries: int) -> None:
    root.mkdir(parents=True, exist_ok=True)
    payload = b"Kite startup fixture\n"
    for index in range(entries):
        (root / f"item-{index:06}.txt").write_bytes(payload)


def count_entries(path: Path) -> int:
    if not path.is_dir():
        return 0
    return sum(1 for child in path.iterdir() if child.is_file())


def platform_executable(app: Path) -> Path:
    if sys.platform == "darwin":
        return app / "Contents" / "MacOS" / "Kite" if app.suffix == ".app" else app
    if sys.platform == "win32":
        return app
    raise RuntimeError("startup measurement supports macOS and Windows only")


def read_trace(path: Path) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        return events
    for line in lines:
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            events.append(value)
    return events


def host_info() -> dict[str, Any]:
    cpu_model = platform.processor() or os.environ.get("PROCESSOR_IDENTIFIER", "unknown")
    memory_bytes: int | None = None
    if sys.platform == "darwin":
        cpu_result = subprocess.run(
            ["sysctl", "-n", "machdep.cpu.brand_string"],
            check=False,
            capture_output=True,
            text=True,
        )
        memory_result = subprocess.run(
            ["sysctl", "-n", "hw.memsize"],
            check=False,
            capture_output=True,
            text=True,
        )
        version_result = subprocess.run(
            ["sw_vers", "-productVersion"],
            check=False,
            capture_output=True,
            text=True,
        )
        if cpu_result.stdout.strip():
            cpu_model = cpu_result.stdout.strip()
        if memory_result.stdout.strip().isdigit():
            memory_bytes = int(memory_result.stdout.strip())
        os_version = version_result.stdout.strip() or platform.release()
    elif sys.platform == "win32":
        class MemoryStatus(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.c_ulong),
                ("dwMemoryLoad", ctypes.c_ulong),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        status = MemoryStatus()
        status.dwLength = ctypes.sizeof(status)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
            memory_bytes = int(status.ullTotalPhys)
        os_version = platform.version()
    else:
        os_version = platform.release()

    return {
        "os": sys.platform,
        "os_version": os_version,
        "architecture": platform.machine(),
        "cpu_model": cpu_model,
        "memory_bytes": memory_bytes,
    }


class ProcessMemory:
    def __init__(self, pid: int) -> None:
        self.pid = pid
        self._windows_handle: int | None = None

    def read(self) -> int | None:
        if sys.platform == "darwin":
            return self._read_macos()
        if sys.platform == "win32":
            return self._read_windows()
        return None

    def close(self) -> None:
        if sys.platform == "win32" and self._windows_handle:
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
            kernel32.CloseHandle(self._windows_handle)
            self._windows_handle = None

    def _read_macos(self) -> int | None:
        result = subprocess.run(
            ["ps", "-o", "rss=", "-p", str(self.pid)],
            check=False,
            capture_output=True,
            text=True,
        )
        value = result.stdout.strip()
        if result.returncode != 0 or not value:
            return None
        try:
            return int(value) * 1024
        except ValueError:
            return None

    def _read_windows(self) -> int | None:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_bool, ctypes.c_uint32]
        if self._windows_handle is None:
            self._windows_handle = kernel32.OpenProcess(
                0x1000 | 0x0010, False, self.pid
            )
            if not self._windows_handle:
                return None

        class ProcessMemoryCounters(ctypes.Structure):
            _fields_ = [
                ("cb", ctypes.c_ulong),
                ("PageFaultCount", ctypes.c_ulong),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        counters = ProcessMemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        psapi.GetProcessMemoryInfo.argtypes = [
            ctypes.c_void_p,
            ctypes.POINTER(ProcessMemoryCounters),
            ctypes.c_uint32,
        ]
        if not psapi.GetProcessMemoryInfo(
            self._windows_handle,
            ctypes.byref(counters),
            counters.cb,
        ):
            return None
        return int(counters.PeakWorkingSetSize)


def terminate_process(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    if sys.platform == "win32":
        process.terminate()
    else:
        process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def run_once(
    executable: Path,
    fixture: Path,
    target_event: str,
    timeout_seconds: float,
) -> dict[str, Any]:
    trace_path = Path(tempfile.mkdtemp(prefix="kite-startup-trace-")) / "trace.jsonl"
    env = os.environ.copy()
    env["KITE_PERF_TRACE_PATH"] = str(trace_path)
    env["KITE_PERF_START_PATH"] = str(fixture)
    argv = [str(executable)]
    if sys.platform == "win32":
        argv.append(str(fixture))

    started_at_ms = time.time_ns() // 1_000_000
    process = subprocess.Popen(
        argv,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=str(executable.parent),
    )
    memory = ProcessMemory(process.pid)
    peak_rss = 0
    target: dict[str, Any] | None = None
    events: list[dict[str, Any]] = []
    deadline = time.monotonic() + timeout_seconds
    try:
        while time.monotonic() < deadline:
            events = read_trace(trace_path)
            target = next((event for event in events if event.get("event") == target_event), None)
            rss = memory.read()
            if rss:
                peak_rss = max(peak_rss, rss)
            if target is not None:
                break
            if process.poll() is not None:
                break
            time.sleep(0.02)
        if target is None:
            raise RuntimeError(
                f"target event {target_event!r} was not observed; process_exit={process.poll()}"
            )
        entry = next(
            (event for event in events if event.get("event") == "process_entry"),
            None,
        )
        event_timestamp = int(target["timestamp_unix_ms"])
        result = {
            "status": "ok",
            "pid": process.pid,
            "version": target.get("version", "unknown"),
            "launch_to_first_content_ms": max(0, event_timestamp - started_at_ms),
            "entry_to_first_content_ms": float(target.get("elapsed_ms", 0)),
            "launch_to_process_entry_ms": (
                max(0, int(entry["timestamp_unix_ms"]) - started_at_ms) if entry else None
            ),
            "peak_rss_bytes": peak_rss or None,
            "events": events,
        }
    finally:
        memory.close()
        terminate_process(process)
        shutil.rmtree(trace_path.parent, ignore_errors=True)
    return result


def build_report(args: argparse.Namespace, fixture: Path) -> dict[str, Any]:
    executable = platform_executable(args.app.resolve())
    if not executable.is_file():
        raise RuntimeError(f"release executable not found: {executable}")
    target_event = args.target_event or TARGET_EVENTS.get(sys.platform)
    if not target_event:
        raise RuntimeError(f"no target event configured for {sys.platform}")

    runs: list[dict[str, Any]] = []
    errors: list[str] = []
    for index in range(args.samples):
        try:
            run = run_once(executable, fixture, target_event, args.timeout)
            run["run_index"] = index
            run["phase"] = "first_process" if index == 0 else "warm_restart"
            runs.append(run)
        except Exception as error:  # noqa: BLE001 - report all startup failures
            errors.append(f"run {index}: {error}")
            runs.append(
                {
                    "run_index": index,
                    "phase": "first_process" if index == 0 else "warm_restart",
                    "status": "error",
                    "error": str(error),
                }
            )

    successful = [run for run in runs if run["status"] == "ok"]
    latency = [float(run["launch_to_first_content_ms"]) for run in successful]
    entry_latency = [float(run["entry_to_first_content_ms"]) for run in successful]
    rss_values = [
        int(run["peak_rss_bytes"])
        for run in successful
        if run.get("peak_rss_bytes") is not None
    ]
    versions = sorted({str(run.get("version", "unknown")) for run in successful})
    return {
        "schema_version": 1,
        "generated_at_unix_ms": time.time_ns() // 1_000_000,
        "platform": sys.platform,
        "platform_release": platform.release(),
        "platform_machine": platform.machine(),
        "host": host_info(),
        "app_name": args.app.name,
        "app_versions": versions,
        "profile": "startup",
        "fixture_entries": count_entries(fixture),
        "target_event": target_event,
        "method": {
            "launch": "new process for every sample",
            "first_process": "run 0 starts after fixture creation",
            "warm_restart": "later runs reuse OS caches",
            "cache_control": "OS caches are not purged",
            "time_origin": "wall clock immediately before process creation",
            "first_content": {
                "darwin": "first SwiftUI KiteColumnBrowser onAppear after the initial directory is loaded",
                "win32": "first KiteApp render after the initial directory load completes",
            }.get(sys.platform, target_event),
            "rss": "process resident memory polled every 20 ms; maximum observed value",
        },
        "summary": {
            "launch_to_first_content": summarize(latency),
            "entry_to_first_content": summarize(entry_latency),
            "peak_rss_bytes_max": max(rss_values) if rss_values else None,
            "phase_summaries": {
                phase: phase_summary(
                    [run for run in successful if run["phase"] == phase]
                )
                for phase in ("first_process", "warm_restart")
            },
        },
        "runs": runs,
        "errors": errors,
    }


def render_markdown(report: dict[str, Any]) -> str:
    def metric(name: str, field: str) -> str:
        summary = report["summary"][name]
        if not summary.get("sample_count"):
            return "-"
        return f"{summary[field]:.3f}"

    lines = [
        "# Kite startup performance",
        "",
        f"- Platform: `{report['platform']}`",
        f"- App: `{report['app_name']}`",
        f"- Versions: `{', '.join(report['app_versions']) or 'unknown'}`",
        f"- Fixture entries: `{report['fixture_entries']}`",
        f"- Target event: `{report['target_event']}`",
        f"- OS: `{report['host']['os_version']}` (`{report['host']['architecture']}`)",
        f"- CPU: `{report['host']['cpu_model']}`",
        f"- Memory: `{report['host']['memory_bytes'] or 0}` bytes",
        "",
        "| Metric | p50 ms | p95 ms | max ms | n |",
        "| --- | ---: | ---: | ---: | ---: |",
        f"| Launch to first content | {metric('launch_to_first_content', 'p50_ms')} | {metric('launch_to_first_content', 'p95_ms')} | {metric('launch_to_first_content', 'max_ms')} | {report['summary']['launch_to_first_content'].get('sample_count', 0)} |",
        f"| Process entry to first content | {metric('entry_to_first_content', 'p50_ms')} | {metric('entry_to_first_content', 'p95_ms')} | {metric('entry_to_first_content', 'max_ms')} | {report['summary']['entry_to_first_content'].get('sample_count', 0)} |",
        "",
        f"Peak RSS: `{report['summary']['peak_rss_bytes_max'] or 0}` bytes.",
        "",
        "### By phase",
        "",
        "| Phase | n | Launch p50 ms | Launch p95 ms | Entry p50 ms | Entry p95 ms |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for phase in ("first_process", "warm_restart"):
        summary = report["summary"]["phase_summaries"][phase]
        lines.append(
            f"| {phase} | {summary['launch_to_first_content'].get('sample_count', 0)} | "
            f"{summary['launch_to_first_content'].get('p50_ms', 0):.3f} | "
            f"{summary['launch_to_first_content'].get('p95_ms', 0):.3f} | "
            f"{summary['entry_to_first_content'].get('p50_ms', 0):.3f} | "
            f"{summary['entry_to_first_content'].get('p95_ms', 0):.3f} |"
        )
    lines.extend([
        "",
        "## Runs",
        "",
        "| Run | Phase | Launch to content ms | Entry to content ms | Peak RSS bytes |",
        "| ---: | --- | ---: | ---: | ---: |",
    ])
    for run in report["runs"]:
        if run["status"] != "ok":
            lines.append(
                f"| {run['run_index']} | {run['phase']} | error | error | error |"
            )
            continue
        lines.append(
            f"| {run['run_index']} | {run['phase']} | {run['launch_to_first_content_ms']:.3f} | {run['entry_to_first_content_ms']:.3f} | {run['peak_rss_bytes'] or 0} |"
        )
    lines.extend(
        [
            "",
            "## Boundary",
            "",
            f"- {report['method']['first_content']}.",
            "- The result is a first-content proxy, not a compositor-confirmed first frame.",
            "- OS caches are not purged. Run 0 is the first process only; later runs use warm caches.",
            "- Native interaction frame stalls remain outside this runner.",
        ]
    )
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True, type=Path, help="Kite.app or kite-explorer.exe")
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--entries", type=int, default=10_000)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--keep-fixture", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--target-event")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be greater than zero")
    if args.entries < 1:
        parser.error("--entries must be greater than zero")
    if args.timeout <= 0:
        parser.error("--timeout must be greater than zero")
    return args


def main() -> int:
    args = parse_args()
    fixture_created = False
    fixture = args.fixture
    if fixture is None:
        fixture = Path(tempfile.mkdtemp(prefix="kite-startup-fixture-"))
        create_fixture(fixture, args.entries)
        fixture_created = True
    elif not fixture.exists():
        create_fixture(fixture, args.entries)
        fixture_created = True
    elif not fixture.is_dir():
        raise SystemExit(f"fixture is not a directory: {fixture}")

    try:
        report = build_report(args, fixture)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"Kite startup report: {args.output}")
        if args.markdown:
            args.markdown.parent.mkdir(parents=True, exist_ok=True)
            args.markdown.write_text(render_markdown(report), encoding="utf-8")
            print(f"Kite startup summary: {args.markdown}")
        if report["errors"]:
            for error in report["errors"]:
                print(error, file=sys.stderr)
            return 1
        return 0
    finally:
        if fixture_created and not args.keep_fixture:
            shutil.rmtree(fixture, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
