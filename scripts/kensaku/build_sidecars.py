#!/usr/bin/env python3
"""Build pinned Kensaku CLI and daemon sidecars for Kite."""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

COMMIT = "919775155b24794f88171a3e29fa44e029d00536"
RELEASE = "v1.0.16"
SOURCE_URL = f"https://codeload.github.com/yjid/kensaku/tar.gz/{COMMIT}"
CLI_ASSETS = {
    "windows-x86_64": (
        "kensaku-cli-windows-x86_64.exe",
        "3fa9ef4721f3d7aafd275f552a1d4a5a1183403891302606f1f4dea787bf1ce0",
    ),
    "linux-x86_64": (
        "kensaku-cli-linux-x86_64",
        "4bd357bbc632fe87bb7be2ad21107ea8911ac7c712e9ac3c74ea2438a24b8dfa",
    ),
    "macos-aarch64": (
        "kensaku-cli-macos-aarch64",
        "3cf5c248dbe8268d32995cc755dd123292442ed738e95fdbc83e41744bc4738b",
    ),
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "Kite-Kensaku-Builder"})
    with urllib.request.urlopen(request, timeout=60) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)


def extract_source(archive: Path, destination: Path) -> Path:
    marker = destination / ".extracted"
    if marker.is_file():
        children = [child for child in destination.iterdir() if child.is_dir()]
        if children:
            return children[0]
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as source:
        root = destination.resolve()
        for member in source.getmembers():
            target = (destination / member.name).resolve()
            if root != target and root not in target.parents:
                raise RuntimeError(f"unsafe archive path: {member.name}")
        source.extractall(destination)
    marker.write_text(COMMIT, encoding="utf-8")
    roots = [child for child in destination.iterdir() if child.is_dir()]
    if len(roots) != 1:
        raise RuntimeError("unexpected Kensaku source archive layout")
    return roots[0]


def target_key() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    if system == "windows" and machine in {"amd64", "x86_64"}:
        return "windows-x86_64"
    if system == "darwin" and machine in {"arm64", "aarch64"}:
        return "macos-aarch64"
    if system == "linux" and machine in {"amd64", "x86_64"}:
        return "linux-x86_64"
    raise RuntimeError(f"unsupported sidecar platform: {system}/{machine}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="vendor/kensaku")
    parser.add_argument("--work", default="target/kensaku-build")
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    output = (root / args.output).resolve()
    work = (root / args.work).resolve()
    output.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)

    key = target_key()
    asset_name, expected = CLI_ASSETS[key]
    cli_path = output / ("kensaku-cli.exe" if key == "windows-x86_64" else "kensaku-cli")
    cli_url = f"https://github.com/yjid/kensaku/releases/download/{RELEASE}/{asset_name}"
    if not cli_path.is_file() or (expected and sha256(cli_path) != expected):
        print(f"download {cli_url}", flush=True)
        download(cli_url, cli_path)
    if expected and sha256(cli_path) != expected:
        raise RuntimeError("Kensaku CLI SHA-256 mismatch")
    if os.name != "nt":
        cli_path.chmod(0o755)

    archive = work / "kensaku-source.tar.gz"
    if not archive.is_file():
        print(f"download {SOURCE_URL}", flush=True)
        download(SOURCE_URL, archive)
    source = extract_source(archive, work / "source")
    manifest = source / "cli" / "Cargo.toml"
    print(f"build Kensaku daemon from {source}", flush=True)
    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            str(manifest),
            "--release",
            "--locked",
            "--bin",
            "kensaku-daemon",
        ],
        cwd=root,
        check=True,
    )
    daemon_name = "kensaku-daemon.exe" if key == "windows-x86_64" else "kensaku-daemon"
    built = source / "cli" / "target" / "release" / daemon_name
    if not built.is_file():
        raise RuntimeError(f"Kensaku daemon was not built: {built}")
    destination = output / daemon_name
    shutil.copy2(built, destination)
    if os.name != "nt":
        destination.chmod(0o755)
    shutil.copy2(source / "LICENSE", output / "KENSAKU_LICENSE")
    print(f"CLI: {cli_path}", flush=True)
    print(f"daemon: {destination}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"kensaku sidecar build failed: {error}", file=sys.stderr)
        raise SystemExit(1)
