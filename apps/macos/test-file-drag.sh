#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/kite-file-drag-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

xcrun swiftc \
    -parse-as-library \
    -framework AppKit \
    "$ROOT/apps/macos/Sources/KiteFileDragProvider.swift" \
    "$ROOT/apps/macos/Tests/FileDragProviderSmoke.swift" \
    -o "$TMP_DIR/FileDragProviderSmoke"

"$TMP_DIR/FileDragProviderSmoke"
