# Kite v0.6.74

Kite v0.6.74 fixes the Windows settings window crash.

## Windows settings

- Read the main app update status before opening the settings window.
- Pass the initial status snapshot into the settings window.
- Prevent a second read of `KiteApp` during window creation.
- Keep the update-status observer active after window creation.
- Fix issue #83.

## Verification

- Windows debug build.
- Windows release build.
- Windows settings window open and reopen.
- All six settings pages.
- Theme preference save and restore.
- Rust formatting check.
- Rust Core tests.
- Windows Clippy check.
- Windows GPUI CI.
- Linux checks.
- Platform parity.
- Startup performance.
- macOS SwiftUI and AppKit release build.

## Install

- Windows: use `Kite-v0.6.74-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.74-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release fixes the Windows settings window.
macOS receives the shared version bump and release package.
macOS parity: not required
