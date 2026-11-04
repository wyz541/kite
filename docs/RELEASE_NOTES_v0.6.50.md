# Kite v0.6.50

Kite v0.6.50 fixes internal drag behavior and immediate column refresh on macOS.

## Drag and refresh fixes

- Route internal drags through the existing cut and paste commands.
- Use copy and paste when Option is held during an internal drag.
- Refresh every visible column after moving or copying files.
- Preserve sort, filter, grouping, and valid selection state during refresh.
- Refresh the source and destination columns immediately after a transfer.
- Add Navigation > Refresh with the Command-R shortcut.
- Keep the file URL as the primary web drag representation.

## Verification

- macOS drag provider smoke check.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- Manual toolbar refresh check.
- Manual cut, paste, and destination refresh check.
- macOS package and signature verification.
- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- Windows package hash verification.
- Release parity check.

## Install

- Windows: use Kite-v0.6.50-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.50-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
