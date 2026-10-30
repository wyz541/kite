# Kite v0.6.29

Kite v0.6.29 fixes sidebar tree expansion for folders that contain files.

## Sidebar tree

- Show direct child files in expanded folders.
- Show direct child folders in expanded folders.
- Sort child folders before child files.
- Show desktop shortcuts and files when expanding Desktop.
- Expand a collapsed folder when its row is clicked.
- Navigate to the folder after row-click expansion.
- Navigate without toggling when the folder is already expanded.
- Keep the expand arrow visible for empty folders.
- Show “没有项目” for empty folders.
- Keep the 256 direct-child limit.
- Keep background loading and session caching.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Sidebar folder-first ordering test.
- Sidebar file and folder loading test.

## Install

- Windows: use `Kite-v0.6.29-windows-x64.zip`.
- macOS: use `Kite-v0.6.29-macOS-arm64.zip`.
