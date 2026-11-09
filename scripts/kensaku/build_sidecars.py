#!/usr/bin/env python3
"""Build pinned Kensaku CLI, daemon, and ONNX Runtime sidecars for Kite."""

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
import zipfile
from pathlib import Path

COMMIT = "919775155b24794f88171a3e29fa44e029d00536"
RELEASE = "v1.0.16"
SOURCE_URL = f"https://codeload.github.com/yjid/kensaku/tar.gz/{COMMIT}"
ONNX_VERSION = "1.31.0"
ONNX_DOWNLOAD_PREFIX = (
    f"https://github.com/microsoft/onnxruntime/releases/download/v{ONNX_VERSION}/"
)
ONNX_LICENSE_URL = (
    f"https://raw.githubusercontent.com/microsoft/onnxruntime/v{ONNX_VERSION}/LICENSE"
)
ONNX_NOTICES_URL = (
    f"https://raw.githubusercontent.com/microsoft/onnxruntime/v{ONNX_VERSION}/ThirdPartyNotices.txt"
)
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
ONNX_ASSETS = {
    "windows-x86_64": (
        "onnxruntime-win-x64-1.31.0.zip",
        "3958d8a44984160692c1b4ffd0de3cb48f8c39f9d34bb31e433db06e30b82195",
        "onnxruntime-win-x64-1.31.0/lib/onnxruntime.dll",
        "onnxruntime.dll",
    ),
    "linux-x86_64": (
        "onnxruntime-linux-x64-1.31.0.tgz",
        "cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57",
        "onnxruntime-linux-x64-1.31.0/lib/libonnxruntime.so.1.31.0",
        "libonnxruntime.so",
    ),
    "macos-aarch64": (
        "onnxruntime-osx-arm64-1.31.0.tgz",
        "1031b36dd3bdaa8d976a6130d9cf2a05e9be320b93ae804f8427b91dc3432afc",
        "onnxruntime-osx-arm64-1.31.0/lib/libonnxruntime.1.31.0.dylib",
        "libonnxruntime.dylib",
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
    with urllib.request.urlopen(request, timeout=120) as response, destination.open("wb") as output:
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


def extract_archive_member(archive: Path, member_name: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if archive.suffix.lower() == ".zip":
        with zipfile.ZipFile(archive) as source, source.open(member_name) as input_stream:
            with destination.open("wb") as output:
                shutil.copyfileobj(input_stream, output)
        return
    with tarfile.open(archive, "r:gz") as source:
        member = source.getmember(member_name)
        input_stream = source.extractfile(member)
        if input_stream is None:
            raise RuntimeError(f"archive member is not a file: {member_name}")
        with destination.open("wb") as output:
            shutil.copyfileobj(input_stream, output)


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


def find_mt() -> Path:
    executable = shutil.which("mt.exe")
    if executable:
        return Path(executable)
    for variable in ("ProgramFiles(x86)", "ProgramFiles"):
        root = os.environ.get(variable)
        if not root:
            continue
        bin_root = Path(root) / "Windows Kits" / "10" / "bin"
        if bin_root.is_dir():
            candidates = sorted(bin_root.glob("*/x64/mt.exe"), reverse=True)
            if candidates:
                return candidates[0]
    raise RuntimeError("Windows SDK mt.exe was not found")


def embed_windows_manifest(daemon: Path) -> None:
    manifest = Path(__file__).with_name("kensaku-daemon.manifest").resolve()
    if not manifest.is_file():
        raise RuntimeError(f"Windows daemon manifest is missing: {manifest}")
    subprocess.run(
        [
            str(find_mt()),
            "-nologo",
            "-manifest",
            str(manifest),
            f"-outputresource:{daemon};#1",
        ],
        check=True,
    )


def stage_onnx_runtime(key: str, work: Path, output: Path) -> Path:
    archive_name, expected, member, output_name = ONNX_ASSETS[key]
    archive = work / archive_name
    if not archive.is_file() or sha256(archive) != expected:
        url = f"{ONNX_DOWNLOAD_PREFIX}{archive_name}"
        print(f"download {url}", flush=True)
        download(url, archive)
    actual = sha256(archive)
    if actual != expected:
        raise RuntimeError(f"ONNX Runtime SHA-256 mismatch: expected {expected}, actual {actual}")
    runtime = output / output_name
    extract_archive_member(archive, member, runtime)
    if not runtime.is_file() or runtime.stat().st_size == 0:
        raise RuntimeError("ONNX Runtime library was not extracted")
    license_path = work / "onnxruntime-LICENSE"
    notices_path = work / "onnxruntime-ThirdPartyNotices.txt"
    if not license_path.is_file():
        download(ONNX_LICENSE_URL, license_path)
    if not notices_path.is_file():
        download(ONNX_NOTICES_URL, notices_path)
    combined = (
        "===== ONNX Runtime LICENSE =====\n"
        + license_path.read_text(encoding="utf-8")
        + "\n\n===== ONNX Runtime ThirdPartyNotices.txt =====\n"
        + notices_path.read_text(encoding="utf-8")
    )
    (output / "ONNXRUNTIME_LICENSE").write_text(combined, encoding="utf-8")
    return runtime


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
    if key == "windows-x86_64":
        embed_windows_manifest(destination)
    if os.name != "nt":
        destination.chmod(0o755)
    runtime = stage_onnx_runtime(key, work, output)
    shutil.copy2(source / "LICENSE", output / "KENSAKU_LICENSE")
    print(f"CLI: {cli_path}", flush=True)
    print(f"daemon: {destination}", flush=True)
    print(f"onnxruntime: {runtime}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"kensaku sidecar build failed: {error}", file=sys.stderr)
        raise SystemExit(1)