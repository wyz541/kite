# Kite v0.6.28

Kite v0.6.28 adds expandable directory trees to the Windows sidebar.

## Sidebar tree

- Show an expand arrow for directory items.
- Click the arrow to expand or collapse the directory.
- Click the name or icon to navigate to the directory.
- Load direct child directories on first expansion.
- Cache child directories for the current application session.
- Show a loading row during child-directory reads.
- Show an error row when child-directory reading fails.
- Hide the arrow after an empty directory is expanded.
- Limit one expansion to 256 direct child directories.
- Support favorites, quick-access folders, removable devices, and drives.
- Keep favorite files clickable without an expand arrow.
- Add vertical scrolling to the Windows sidebar.

## Stability

- Run child-directory reads in the background.
- Filter ordinary files from the directory tree.
- Keep right-click menus independent from tree expansion.
- Keep the current column navigation rules unchanged.
- Keep tree state out of the favorites configuration.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Direct-child directory loading test.
- Ordinary-file filtering test.

## Install

- Windows: use `Kite-v0.6.28-windows-x64.zip`.
- macOS: use `Kite-v0.6.28-macOS-arm64.zip`.
