# Kite v0.6.49

Kite v0.6.49 fixes macOS file drag-and-drop regressions.

## Drag-and-drop fixes

- Use the system file URL as the primary drag representation for web upload targets.
- Keep the Kite internal drag payload as the secondary representation.
- Support web upload targets that reject text-only drag data.
- Open the destination directory after a successful drop.
- Keep moved files visible in the destination column.
- Preserve Finder drag-out and internal move or copy behavior.

## Verification

- macOS drag provider smoke check.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- macOS package and signature verification.
- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- Windows package hash verification.
- Release parity check.

## Install

- Windows: use Kite-v0.6.49-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.49-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
