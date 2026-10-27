# Kite v0.6.1

Kite v0.6.1 removes duplicate path navigation entries.

## Navigation cleanup

- Remove the toolbar Go menu.
- Remove the separate Go to Folder modal.
- Remove the `Ctrl+Shift+G` and `Command+Shift+G` path shortcuts.
- Keep the path field as the only direct path entry.
- Submit the path with Enter or the Open button.
- Rename the macOS submit label from Go to Open.
- Keep one parent navigation button in the toolbar.
- Keep the Windows parent shortcut as `Alt+Up`.
- Keep the macOS parent shortcut as `Command+Up`.
- Change the menu bar Go menu to a Navigation menu.
- Keep only Focus Path in the Navigation menu.
- Add `Command+L` for macOS path focus.
- Keep `Ctrl+L` for Windows path focus.
- Keep tab and session restoration behavior unchanged.

## macOS screenshot

![Kite macOS navigation](https://github.com/wyz541/kite/releases/download/v0.6.1/Kite-v0.6.1-macOS-navigation.png)

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- macOS VCS menus are not yet complete.
- macOS Finder Sync is not yet implemented.
- macOS Universal Binary is not yet implemented.
- macOS notarization is not yet implemented.

## Installation notes

- Windows builds are not code-signed. Windows SmartScreen can show a warning.
- The macOS build is arm64 and ad-hoc signed. The app is not notarized. macOS Gatekeeper can block the first launch.

## Install

- Windows: use `Kite-v0.6.1-windows-x64.zip`.
- macOS: use `Kite-v0.6.1-macOS-arm64.zip`.