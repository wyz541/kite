# Kite v0.6.68

Kite v0.6.68 fixes live global search while Kensaku builds the first index.

## Live search

- Query the Kensaku daemon directly with JSON-RPC.
- Use live daemon results during the first scan.
- Fall back to the persisted index through `kensaku-cli` only when daemon IPC is unavailable.
- Show the current scan, extraction, or commit phase.
- Refresh the active query every 2 seconds while Kensaku indexes.
- Preserve current results during an automatic refresh.

## Verification

- Rust formatting check.
- Core tests.
- Clippy with warnings denied.
- Windows UI build check.
- Live daemon IPC contract test.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.68-windows-x64.zip`.
- Windows direct executable: use `Kite.exe` only to update from v0.6.67. Extract the ZIP for a new installation.
- macOS: use `Kite-v0.6.68-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

The shared Rust Core change is built on Windows and macOS.
No Swift UI source changed.
macOS parity: not required
