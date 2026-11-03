# Kite v0.6.45

Kite v0.6.45 adds file and folder drag and drop on Windows and macOS.

## Drag and drop

- Drag files and folders from column rows.
- Drop onto folder rows or column backgrounds.
- Move items by default during internal drops.
- Copy internal drops with Ctrl on Windows or Option on macOS.
- Copy files and folders dragged in from Explorer or Finder.
- Drag files and folders to Explorer, Finder, and other applications.
- Show drop-target feedback on folders and columns.
- Reject unsupported portable-device drag operations with a clear message.

## Platform integration

- Windows uses GPUI internal drag events and native OLE file drag-out.
- macOS uses `NSItemProvider` with Kite internal drag data and system file URLs.
- Both platforms reuse the shared transfer implementation.
- Folder moves and copies validate destination paths before transfer.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows OLE drag module cross-check.
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

- Windows: use Kite-v0.6.45-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.45-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
