# Kite v0.6.61

Kite v0.6.61 uses the macOS system volume name for the root global-search scope.

## macOS global search

- Read the root volume display name from `/`.
- Show `Macintosh HD` on the current system.
- Fall back to `Macintosh HD` if macOS returns no usable name.
- Keep `/` as the scope summary.
- Preserve the native selected state, icons, and disabled reasons.

## Windows global search

- Keep the existing Windows search scopes and behavior.
- The Windows package is rebuilt from the same release commit.

## Platform parity

- macOS defines the global-search scope-selector baseline.
- Windows keeps the same information hierarchy through GPUI.
- This release changes only the macOS root-volume label.

## Verification

- Rust formatting check.
- Rust Core tests.
- macOS Clippy.
- Windows Clippy.
- Windows release build.
- Windows Chrome and Edge drag-upload E2E.
- macOS SwiftUI and AppKit release build.
- macOS application signature verification.
- macOS root-scope menu visual check.
- Feature matrix check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.61-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.61-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
