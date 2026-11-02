# Kite v0.6.37

Kite v0.6.37 fixes stale update-check status in the Windows status bar.

## Update status

- Clear the update-check result after the dialog closes.
- Show the detected version while the update dialog is open.
- Clear the detected version when the user chooses "Later".
- Prevent an old "up to date" message from remaining after a new Release is published.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.

## Install

- Windows: use `Kite-v0.6.37-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.37-macOS-arm64.zip`.
- Verify all downloaded files with `SHA256SUMS.txt`.
