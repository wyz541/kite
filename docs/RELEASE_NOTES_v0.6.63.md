# Kite v0.6.63

Kite v0.6.63 replaces Kite’s global search index with Kensaku.
Global name and content search now use the same local Kensaku daemon.

## Global search

- Start and manage `kensaku-daemon` from Kite.
- Let the daemon own the first scan, filesystem watch, incremental update, and content extraction.
- Use `kensaku-cli` for read-only global name and content queries.
- Pass Kite search roots into Kensaku configuration.
- Restart the daemon after adding an uncovered root.
- Keep index data in a Kite-specific data directory.
- Show Kensaku index progress in the global search window.

## Removed Kite indexes

- Remove the Kite persistent name index.
- Remove Windows USN Journal index updates.
- Remove Spotlight and Everything global-search fallback paths.
- Remove the Kite content index writer and watcher.
- Remove the Kite global filesystem traversal fallback.
- Remove the Tantivy dependency from Kite Core.

## Sidecars

- Pin Kensaku `v1.0.16` and source commit `9197751`.
- Add `scripts/kensaku/build_sidecars.py`.
- Verify the Kensaku CLI SHA-256.
- Build the Kensaku daemon from pinned source.
- Package the CLI and daemon beside Kite.
- Add Windows and macOS sidecar packaging.
- Retain the Kensaku MIT license.

## Search behavior

- Name mode sends `name:` queries to Kensaku.
- Content mode sends text or regex queries to Kensaku.
- Kite applies type and time filters before final result limits.
- Kite converts Kensaku snippets into content result rows.
- Kite does not fall back to a full-disk traversal.

## Verification

- Rust formatting check.
- Rust Core tests.
- Clippy with warnings denied.
- Windows UI build check.
- Performance harness build.
- UniFFI Swift binding generation.
- Windows and macOS build-script syntax checks.
- Fake daemon lifecycle and query contract test.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.63-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS Apple Silicon: use `Kite-v0.6.63-macOS-arm64.zip`.
- Verify downloads with `SHA256SUMS.txt`.

## Limit

- First launch initializes the Kensaku index in the background.
- Search results are partial until the Kensaku daemon reports a usable index.
