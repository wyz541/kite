#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

echo "Checking Rust formatting..."
cargo fmt --all -- --check

echo "Running core tests..."
cargo test --locked --lib

echo "Running Clippy..."
cargo clippy --locked --all-targets -- -D warnings

echo "Testing directory watcher..."
bash apps/macos/test-directory-watcher.sh

echo "Testing file drag provider..."
bash apps/macos/test-file-drag.sh

echo "Building release app..."
bash apps/macos/build.sh

echo "macOS verification passed."
