# Kite v0.6.27

Kite v0.6.27 adds persistent favorites.

## Favorites

- Add “Add to Favorites” to the Windows context menu.
- Add a Windows favorites section to the sidebar.
- Add “Add to Favorites” to macOS directory and entry menus.
- Add a macOS favorites bar below the toolbar.
- Open favorite folders in Kite.
- Open favorite files with the system default application.
- Remove favorites from the favorites section.
- Deduplicate saved paths.
- Persist favorites across application restarts.
- Store favorites in a shared JSON configuration.
- Limit the configuration to 128 entries.
- Fall back to the backup file when the main file is corrupt.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Favorites add test.
- Favorites deduplication test.
- Favorites remove test.
- Favorites backup recovery test.
- Context menu target test.

## Install

- Windows: use `Kite-v0.6.27-windows-x64.zip`.
- macOS: use `Kite-v0.6.27-macOS-arm64.zip`.
