# Kite v0.6.66

Kite v0.6.66 fixes the global search hang after Kensaku daemon startup.

## Global search

- Stop running `kensaku-daemon --help` during daemon discovery.
- Check daemon file existence without starting the daemon.
- Keep CLI `--help` validation.
- Start the daemon only after discovery completes.

## Verification

- Rust formatting check.
- Core tests.
- Clippy with warnings denied.
- Windows UI build check.
- Daemon discovery regression test.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.66-windows-x64.zip`.
- Windows direct executable: use `Kite.exe` only to update from v0.6.65. Extract the ZIP for a new installation.
- macOS: use `Kite-v0.6.66-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release has no macOS UI source change.
macOS parity: not required