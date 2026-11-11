#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

ALLOWED_IMAGES = {
    "assets/icon/kite-icon.svg",
    "assets/kite.icns",
    "assets/kite.ico",
    "assets/kite-icon-master.png",
    "assets/kite-icon-preview.png",
}
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp", ".bmp"}
ALLOWED_WINDOWS_USERS = {"Example", "Kite", "Public", "runner", "runneradmin", "<name>"}
ALLOWED_UNIX_USERS = {"Example", "example", "<name>", "runner", "Shared"}
WINDOWS_USER = re.compile(r"C:\\Users\\([^\\\s\"'<>]+)")
UNIX_USER = re.compile(r"/Users/([^/\s\"'<>]+)")
PERSONAL_EMAIL = re.compile(
    r"\b[A-Za-z0-9._%+-]+@(?:gmail|qq|163|126|outlook|hotmail)\.[A-Za-z]{2,}\b",
    re.IGNORECASE,
)
ALLOWED_CPU = {"Generic CPU", "Generic Apple Silicon", "Generic x64 CPU"}
ALLOWED_OS = {"unknown", "macOS"}


def tracked_files() -> list[str]:
    output = subprocess.check_output(["git", "ls-files", "-z"])
    return [item for item in output.decode("utf-8").split("\0") if item]


def check_host(data: object, path: str, failures: list[str], seen: set[int]) -> None:
    if id(data) in seen:
        return
    if isinstance(data, dict):
        seen.add(id(data))
        host = data.get("host")
        if isinstance(host, dict):
            cpu = host.get("cpu_model")
            os_version = host.get("os_version")
            if cpu not in ALLOWED_CPU:
                failures.append(f"{path}: personal cpu_model")
            if os_version not in ALLOWED_OS:
                failures.append(f"{path}: personal os_version")
            for key in (
                "logical_cores",
                "memory_bytes",
                "fixture_disk_total_bytes",
                "fixture_disk_available_bytes",
            ):
                if host.get(key) not in (None, 0):
                    failures.append(f"{path}: personal host field {key}")
        for value in data.values():
            check_host(value, path, failures, seen)
    elif isinstance(data, list):
        seen.add(id(data))
        for value in data:
            check_host(value, path, failures, seen)


def main() -> int:
    failures: list[str] = []
    for rel in tracked_files():
        path = Path(rel)
        suffix = path.suffix.lower()
        if suffix in IMAGE_SUFFIXES or suffix == ".svg":
            if rel not in ALLOWED_IMAGES:
                failures.append(f"{rel}: tracked screenshot or unreviewed image")
            continue
        if rel == "Cargo.lock":
            continue
        try:
            raw = path.read_bytes()
        except OSError as error:
            failures.append(f"{rel}: cannot read: {error}")
            continue
        if b"\0" in raw:
            continue
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(text.splitlines(), 1):
            for match in WINDOWS_USER.finditer(line):
                if match.group(1) not in ALLOWED_WINDOWS_USERS:
                    failures.append(f"{rel}:{line_number}: personal Windows user path")
            for match in UNIX_USER.finditer(line):
                if match.group(1) not in ALLOWED_UNIX_USERS:
                    failures.append(f"{rel}:{line_number}: personal macOS user path")
            if PERSONAL_EMAIL.search(line):
                failures.append(f"{rel}:{line_number}: personal email address")
        if rel.startswith("docs/performance/") and suffix == ".json":
            try:
                check_host(json.loads(text), rel, failures, set())
            except json.JSONDecodeError as error:
                failures.append(f"{rel}: invalid JSON: {error}")

    if failures:
        print("Privacy check failed:")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print("Privacy check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())