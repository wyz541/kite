# Kite v0.6.0

Kite v0.6.0 restores tabs and directories after relaunch.

## Session restore

- Save session state when tabs or directories change.
- Save session state when the window closes.
- Save the active tab and tab order.
- Save each tab's current directory.
- Save list or column mode.
- Save path bar visibility.
- Restore visible parent columns from each saved directory.
- Skip missing or inaccessible directories.
- Fall back to the default directory when no saved tab is usable.
- Use a previous session backup when the primary file is corrupt.
- Write session files through a temporary file.
- Keep an explicit Windows launch path ahead of saved state.

## macOS

- Add a native SwiftUI tab bar.
- Add new, close, activate, next, and previous tab actions.
- Add `Command+T` and `Command+W`.
- Add `Control+Tab` and `Control+Shift+Tab`.
- Save tabs on window close and app termination.
- Restore tabs with native SwiftUI column chains.
- Keep Liquid Glass on macOS 26.

## Windows

- Restore all existing GPUI tabs.
- Save state on tab and directory changes.
- Save state through the GPUI window close hook.

## Storage

- Windows: `%LOCALAPPDATA%\Kite\session.json`.
- macOS: `~/Library/Application Support/Kite/session.json`.
- Kite keeps a `.bak` session backup.

## Not saved

- Search queries.
- Scroll positions.
- Selected files.
- Sort order.
- Clipboard contents.
- Window position and size.

## macOS screenshot

![Kite macOS tabs](https://github.com/wyz541/kite/releases/download/v0.6.0/Kite-v0.6.0-macOS-tabs.png)

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows close and two-tab restore smoke test.
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

- Windows: use `Kite-v0.6.0-windows-x64.zip`.
- macOS: use `Kite-v0.6.0-macOS-arm64.zip`.