# Kite v0.6.69

Kite v0.6.69 removes Kensaku and restores Kite's built-in global index.

## Global search

- Restore the persistent name index.
- Restore Windows USN Journal updates.
- Restore the content index and query verification.
- Restore name and content filters.
- Restore local scan fallback while the first index is incomplete.
- Remove the background search daemon and command-line sidecar.
- Remove runtime sidecar downloads and updater integration.
- Remove search daemon licenses and configuration.

## Upgrade

- The application no longer starts or queries the removed search daemon.
- The release package contains only Kite files.
- Old search daemon data is not read.

## Verification

- Rust formatting check.
- Core tests.
- Content search tests.
- Windows USN Journal tests.
- Clippy with warnings denied.
- Windows UI build check.
- macOS build check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.69-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.69-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

The shared search implementation changed on Windows and macOS.
macOS parity: required
