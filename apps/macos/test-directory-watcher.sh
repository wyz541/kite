#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/kite-directory-watcher-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

xcrun swiftc \
    -parse-as-library \
    -framework Foundation \
    "$ROOT/apps/macos/Sources/DirectoryWatcher.swift" \
    "$ROOT/apps/macos/Tests/DirectoryWatcherSmoke.swift" \
    -o "$TMP_DIR/DirectoryWatcherSmoke"

"$TMP_DIR/DirectoryWatcherSmoke"
