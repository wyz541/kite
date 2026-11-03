# Kite v0.6.46

Kite v0.6.46 fixes complete file and folder dragging on Windows and macOS.

## Drag-out fixes

- macOS registers the real file URL before the internal drag data.
- Finder now receives complete files and folders instead of JSON-backed files.
- Windows creates a native Shell data object from file and folder identifiers.
- Explorer now receives complete file system items.
- Windows drag-out no longer replaces the system clipboard.
- Internal move and copy behavior remains unchanged.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows Shell drag module cross-check.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- macOS drag provider round-trip check.
- Release parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.46-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.46-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
