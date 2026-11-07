# Kite v0.6.62

Kite v0.6.62 adds reproducible performance measurement for Windows and macOS.
The release does not change normal file-manager behavior.

## Performance measurement

- Add deterministic Core benchmarks for directory loading, sorting, name search, content search, file operations, archives, VCS status, and update hashing.
- Add JSON and Markdown reports with p50, p95, max, RSS, CPU, I/O, and throughput.
- Add three-run aggregation with per-scenario noise thresholds.
- Add comparison tooling for release-to-release performance changes.
- Publish the first Windows and macOS performance baselines.

## Startup measurement

- Add opt-in JSONL performance tracing on Windows and macOS.
- Measure release process startup to first content.
- Record time to process entry, first window render, first directory, and first content.
- Add Windows and macOS startup workflows with retained raw samples.
- Keep tracing disabled unless `KITE_PERF_TRACE_PATH` is set.

## Platform parity

- Windows and macOS produce the same trace schema.
- Windows uses GPUI first-content rendering.
- macOS uses SwiftUI first-content appearance.
- Both platforms retain platform-specific benchmark boundaries.

## Verification

- Rust formatting check.
- Rust Core tests.
- Feature matrix check.
- Windows Clippy.
- Windows release build.
- Windows Chrome and Edge drag-upload E2E.
- macOS Clippy.
- macOS SwiftUI and AppKit release build.
- Windows performance workflow.
- macOS performance workflow.
- Windows startup measurement.
- macOS startup measurement.
- Release parity check.
- Version consistency check.

## Install

- Windows: use `Kite-v0.6.62-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.62-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
