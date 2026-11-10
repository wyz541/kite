# Kite v0.6.72

Kite v0.6.72 fixes Windows update verification on systems without the `Get-FileHash` PowerShell module.

## Windows updater

- Replace `Get-FileHash` with a .NET SHA-256 implementation.
- Verify the copied `Kite.exe` without loading a PowerShell utility module.
- Keep the existing update retry, rollback, and restart behavior.
- Add a regression test that rejects `Get-FileHash` in the update helper.
- Document the module-independent verification path.

## Upgrade note

Kite v0.6.71 and earlier use the old update helper.
If an update fails at the PowerShell hash step, install v0.6.72 manually once.
Updates started from v0.6.72 use the fixed helper.

## Verification

- Targeted updater regression test.
- Windows PowerShell helper syntax test.
- Windows Clippy check.
- Windows release build.
- Windows Chrome and Edge drag-and-drop E2E.
- Rust Core tests.
- macOS SwiftUI and AppKit release build.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.72-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.72-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release fixes the Windows updater only.
macOS receives the shared version bump and release package.
macOS parity: not required
