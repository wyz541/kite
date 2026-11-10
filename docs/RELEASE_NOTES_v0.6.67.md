# Kite v0.6.67

Kite v0.6.67 fixes empty global-search results after Kensaku starts.

## Configuration repair

- Preserve the existing Kensaku configuration.
- Add missing `excluded_dirs` and `excluded_extensions`.
- Add missing shortcut and result-limit fields.
- Set the pinned Kensaku configuration version.
- Prevent Kensaku from treating Kite configuration as corrupt.

## Indexing

- Keep the selected Kite search roots.
- Build the first Kensaku index with the repaired configuration.
- Keep the configuration valid across later root updates.

## Verification

- Legacy configuration repair test.
- Kensaku configuration smoke test.
- Rust formatting check.
- Core tests.
- Clippy with warnings denied.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.67-windows-x64.zip`.
- Windows direct executable: use `Kite.exe` only to update from v0.6.66. Extract the ZIP for a new installation.
- macOS: use `Kite-v0.6.67-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release has no macOS UI source change.
macOS parity: not required